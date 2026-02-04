
import importlib.util
import json
import os
import sys
from datetime import datetime, timezone
from unittest.mock import MagicMock, patch

import pytest

_HANDLER_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "handler.py")
_MOD_NAME = "policies_handler"

def _load_handler(mock_s3):
    os.environ["BUCKET_NAME"] = "test-policies-bucket"
    sys.modules.pop(_MOD_NAME, None)

    with patch("boto3.client", return_value=mock_s3):
        spec = importlib.util.spec_from_file_location(_MOD_NAME, _HANDLER_PATH)
        mod = importlib.util.module_from_spec(spec)
        sys.modules[_MOD_NAME] = mod
        spec.loader.exec_module(mod)

    return mod

@pytest.fixture(autouse=True)
def handler_module():
    mock_s3 = MagicMock()
    mock_s3.exceptions.NoSuchKey = type("NoSuchKey", (Exception,), {})

    mod = _load_handler(mock_s3)
    mod._mock_s3 = mock_s3
    yield mod

    sys.modules.pop(_MOD_NAME, None)

class TestListPolicies:

    # lists all yaml policies in the s3 bucket with their device id associations
    def test_returns_policy_summaries(self, handler_module):
        ms3 = handler_module._mock_s3
        now = datetime(2026, 1, 15, tzinfo=timezone.utc)

        ms3.list_objects_v2.return_value = {
            "Contents": [
                {"Key": "compressor-policy.yaml", "LastModified": now, "Size": 512},
                {"Key": "inverter-policy.yaml", "LastModified": now, "Size": 256},
            ]
        }
        ms3.head_object.side_effect = [
            {"Metadata": {"device-ids": "0x0001,0x0002"}},
            {"Metadata": {"device-ids": "0x0003"}},
        ]

        result = handler_module.list_policies()

        assert result["statusCode"] == 200
        body = json.loads(result["body"])
        assert len(body) == 2
        assert body[0]["name"] == "compressor-policy.yaml"
        assert body[0]["deviceIds"] == ["0x0001", "0x0002"]
        assert body[1]["name"] == "inverter-policy.yaml"
        assert body[1]["deviceIds"] == ["0x0003"]

    # empty bucket scenario, the factory hasn't uploaded any policies yet
    def test_returns_empty_list_when_bucket_empty(self, handler_module):
        ms3 = handler_module._mock_s3
        ms3.list_objects_v2.return_value = {}

        result = handler_module.list_policies()

        assert result["statusCode"] == 200
        assert json.loads(result["body"]) == []

    # filters policies so the desktop app only shows the ones for device 0x0001
    def test_filters_by_device_id(self, handler_module):
        ms3 = handler_module._mock_s3
        now = datetime(2026, 1, 15, tzinfo=timezone.utc)

        ms3.list_objects_v2.return_value = {
            "Contents": [
                {"Key": "policy-a.yaml", "LastModified": now, "Size": 100},
                {"Key": "policy-b.yaml", "LastModified": now, "Size": 200},
            ]
        }
        ms3.head_object.side_effect = [
            {"Metadata": {"device-ids": "0x0001"}},
            {"Metadata": {"device-ids": "0x0002"}},
        ]

        result = handler_module.list_policies(device_id="0x0001")

        assert result["statusCode"] == 200
        body = json.loads(result["body"])
        assert len(body) == 1
        assert body[0]["name"] == "policy-a.yaml"

class TestPutPolicy:

    # verifies that the yaml content and device metadata are stored correctly in s3
    def test_stores_policy_with_metadata(self, handler_module):
        ms3 = handler_module._mock_s3
        body = {
            "content": "services:\n  - name: temp\n",
            "deviceIds": ["0x0001", "0x0002"],
        }

        result = handler_module.put_policy("compressor.yaml", body)

        assert result["statusCode"] == 200
        ms3.put_object.assert_called_once()
        kw = ms3.put_object.call_args.kwargs
        assert kw["Key"] == "compressor.yaml"
        assert kw["Body"] == b"services:\n  - name: temp\n"
        assert kw["ContentType"] == "text/yaml"
        assert kw["Metadata"] == {"device-ids": "0x0001,0x0002"}

    # a generic policy with no device associations should still work
    def test_stores_policy_with_empty_device_ids(self, handler_module):
        ms3 = handler_module._mock_s3
        body = {"content": "key: value", "deviceIds": []}

        result = handler_module.put_policy("generic.yaml", body)

        assert result["statusCode"] == 200
        kw = ms3.put_object.call_args.kwargs
        assert kw["Metadata"] == {"device-ids": ""}

class TestDeletePolicy:

    # confirms the s3 delete uses the right bucket and key
    def test_removes_from_s3(self, handler_module):
        ms3 = handler_module._mock_s3

        result = handler_module.delete_policy("old-policy.yaml")

        assert result["statusCode"] == 200
        ms3.delete_object.assert_called_once_with(
            Bucket="test-policies-bucket", Key="old-policy.yaml"
        )
