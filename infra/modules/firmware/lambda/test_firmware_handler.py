
import base64
import importlib.util
import json
import os
import sys
from datetime import datetime, timezone
from io import BytesIO
from unittest.mock import MagicMock, patch

import pytest

_HANDLER_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "handler.py")
MAX_FW_SIZE = 4 * 1024 * 1024
_MOD_NAME = "firmware_handler"

def _load_handler(mock_s3):
    os.environ["BUCKET_NAME"] = "test-firmware-bucket"
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

class TestListFirmware:

    # checks that listing firmware returns the right structure and skips status/ objects
    def test_returns_correct_structure(self, handler_module):
        ms3 = handler_module._mock_s3
        now = datetime(2026, 1, 1, tzinfo=timezone.utc)

        ms3.list_objects_v2.return_value = {
            "Contents": [
                {"Key": "app-v1.bin", "LastModified": now, "Size": 1024},
                {"Key": "status/0x0001/app-v1.bin.json", "LastModified": now, "Size": 128},
            ]
        }
        ms3.head_object.return_value = {
            "Metadata": {"device-ids": "0x0001,0x0002"}
        }

        result = handler_module.list_firmware()

        assert result["statusCode"] == 200
        body = json.loads(result["body"])
        # should only have the .bin file, not the status json
        assert len(body) == 1
        assert body[0]["name"] == "app-v1.bin"
        assert body[0]["size"] == 1024
        assert body[0]["deviceIds"] == ["0x0001", "0x0002"]
        assert "lastModified" in body[0]

    # empty s3 bucket should give back an empty list, not an error
    def test_returns_empty_list_when_bucket_empty(self, handler_module):
        ms3 = handler_module._mock_s3
        ms3.list_objects_v2.return_value = {}

        result = handler_module.list_firmware()

        assert result["statusCode"] == 200
        assert json.loads(result["body"]) == []

class TestPutFirmware:

    # only .bin files are accepted, .hex or .elf should fail
    def test_validates_bin_extension(self, handler_module):
        content = base64.b64encode(b"\x00" * 10).decode()
        body = {"content": content, "deviceIds": []}

        result = handler_module.put_firmware("firmware.hex", body)

        assert result["statusCode"] == 400
        assert "bin" in json.loads(result["body"])["error"].lower()

    # firmware binaries over 4MB are rejected to keep s3 costs down
    def test_validates_max_size(self, handler_module):
        oversized = base64.b64encode(b"\x00" * (MAX_FW_SIZE + 1)).decode()
        body = {"content": oversized, "deviceIds": []}

        result = handler_module.put_firmware("big.bin", body)

        assert result["statusCode"] == 400
        assert "limit" in json.loads(result["body"])["error"].lower()

    # happy path upload, verifies the binary lands in s3 with correct key
    def test_successful_upload(self, handler_module):
        ms3 = handler_module._mock_s3
        raw_bytes = b"\xDE\xAD\xBE\xEF"
        content = base64.b64encode(raw_bytes).decode()
        body = {"content": content, "deviceIds": ["0x0001"]}

        result = handler_module.put_firmware("app.bin", body)

        assert result["statusCode"] == 200
        ms3.put_object.assert_called_once()
        kw = ms3.put_object.call_args.kwargs
        assert kw["Key"] == "app.bin"
        assert kw["Body"] == raw_bytes

    # garbled content should return 400 not 500
    def test_rejects_invalid_base64(self, handler_module):
        body = {"content": "!!!not-base64!!!", "deviceIds": []}

        result = handler_module.put_firmware("app.bin", body)

        assert result["statusCode"] == 400
        assert "base64" in json.loads(result["body"])["error"].lower()

class TestDeleteFirmware:

    # verifies the s3 delete call uses the correct bucket and key
    def test_calls_s3_delete(self, handler_module):
        ms3 = handler_module._mock_s3

        result = handler_module.delete_firmware("old.bin")

        assert result["statusCode"] == 200
        ms3.delete_object.assert_called_once_with(
            Bucket="test-firmware-bucket", Key="old.bin"
        )

class TestPostFirmwareStatus:

    # the gateway sends OTA progress to this endpoint, stored under status/ prefix in s3
    def test_stores_status_in_s3(self, handler_module):
        ms3 = handler_module._mock_s3
        body = {
            "device_id": "0x0007",
            "firmware_name": "app-v2.bin",
            "status": "success",
            "version": "2.0.0",
            "error": "",
            "progress": 100,
        }

        result = handler_module.post_fw_status(body)

        assert result["statusCode"] == 200
        ms3.put_object.assert_called_once()
        kw = ms3.put_object.call_args.kwargs
        assert kw["Key"] == "status/0x0007/app-v2.bin.json"
        stored = json.loads(kw["Body"])
        assert stored["status"] == "success"
        assert stored["device_id"] == "0x0007"

    # missing device_id should be a 400 not an unhandled exception
    def test_rejects_missing_device_id(self, handler_module):
        body = {"firmware_name": "app.bin"}

        result = handler_module.post_fw_status(body)

        assert result["statusCode"] == 400

    # missing firmware_name should also be a 400
    def test_rejects_missing_firmware_name(self, handler_module):
        body = {"device_id": "0x0001"}

        result = handler_module.post_fw_status(body)

        assert result["statusCode"] == 400

class TestGetFirmwareStatus:

    # should return all status records for a given the client device
    def test_returns_status_list(self, handler_module):
        ms3 = handler_module._mock_s3
        sts_obj = {
            "device_id": "0x0007",
            "firmware_name": "app-v2.bin",
            "status": "success",
        }

        ms3.list_objects_v2.return_value = {
            "Contents": [{"Key": "status/0x0007/app-v2.bin.json"}]
        }
        ms3.get_object.return_value = {
            "Body": BytesIO(json.dumps(sts_obj).encode())
        }

        result = handler_module.get_fw_status("0x0007")

        assert result["statusCode"] == 200
        body = json.loads(result["body"])
        assert len(body) == 1
        assert body[0]["status"] == "success"

    # no OTA history yet for this device
    def test_returns_empty_when_no_statuses(self, handler_module):
        ms3 = handler_module._mock_s3
        ms3.list_objects_v2.return_value = {}

        result = handler_module.get_fw_status("0xFFFF")

        assert result["statusCode"] == 200
        assert json.loads(result["body"]) == []
