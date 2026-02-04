
import importlib.util
import json
import os
import sys
from unittest.mock import MagicMock, patch

import pytest

_HANDLER_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "handler.py")
_MOD_NAME = "devices_handler"

def _load_handler(mock_tbl):
    # sets up env and patches boto3 so the handler thinks it's running in lambda
    os.environ["TABLE_NAME"] = "test-devices-table"

    mock_res = MagicMock()
    mock_res.Table.return_value = mock_tbl

    sys.modules.pop(_MOD_NAME, None)

    with patch("boto3.resource", return_value=mock_res):
        spec = importlib.util.spec_from_file_location(_MOD_NAME, _HANDLER_PATH)
        mod = importlib.util.module_from_spec(spec)
        sys.modules[_MOD_NAME] = mod
        spec.loader.exec_module(mod)

    return mod

@pytest.fixture(autouse=True)
def handler_module():
    mock_tbl = MagicMock()
    mod = _load_handler(mock_tbl)
    mod._mock_table = mock_tbl
    yield mod

    sys.modules.pop(_MOD_NAME, None)

class TestListDevices:

    # verifies that scan returns all the client device records from dynamodb
    def test_returns_all_devices(self, handler_module):
        mt = handler_module._mock_table
        devices = [
            {"device_id": "0x0001", "protocol": "modbus"},
            {"device_id": "0x0002", "protocol": "canbus"},
        ]
        mt.scan.return_value = {"Items": devices}

        result = handler_module.list_devices()

        assert result["statusCode"] == 200
        body = json.loads(result["body"])
        assert len(body) == 2
        assert body[0]["device_id"] == "0x0001"
        assert body[1]["device_id"] == "0x0002"

    # when the dynamodb table is empty we should still get a valid 200 with []
    def test_returns_empty_list_when_no_devices(self, handler_module):
        mt = handler_module._mock_table
        mt.scan.return_value = {"Items": []}

        result = handler_module.list_devices()

        assert result["statusCode"] == 200
        assert json.loads(result["body"]) == []

class TestGetDevice:

    # happy path, single compressor lookup by hex device id
    def test_returns_single_device(self, handler_module):
        mt = handler_module._mock_table
        dev = {"device_id": "0x0001", "protocol": "modbus"}
        mt.get_item.return_value = {"Item": dev}

        result = handler_module.get_device("0x0001")

        assert result["statusCode"] == 200
        body = json.loads(result["body"])
        assert body["device_id"] == "0x0001"
        mt.get_item.assert_called_once_with(Key={"device_id": "0x0001"}, ConsistentRead=True)

    # simulates querying a device_id that doesn't exist in the registry
    def test_returns_404_when_not_found(self, handler_module):
        mt = handler_module._mock_table
        mt.get_item.return_value = {}

        result = handler_module.get_device("0xFFFF")

        assert result["statusCode"] == 404
        assert "not found" in json.loads(result["body"])["error"].lower()

class TestPutDevice:

    # creates a new modbus device entry in the the client dynamodb table
    def test_creates_device(self, handler_module):
        mt = handler_module._mock_table
        body = {"protocol": "modbus"}

        result = handler_module.put_device("0x0001", body)

        assert result["statusCode"] == 200
        mt.put_item.assert_called_once()
        kw = mt.put_item.call_args.kwargs
        assert kw["Item"]["device_id"] == "0x0001"
        assert kw["Item"]["protocol"] == "modbus"

    # makes sure the yaml descriptor (service definitions, graph_data, etc) is persisted
    def test_stores_optional_descriptor(self, handler_module):
        mt = handler_module._mock_table
        yaml_str = "services:\n  - name: temp\n"
        body = {"protocol": "modbus", "descriptor": yaml_str}

        result = handler_module.put_device("0x0001", body)

        assert result["statusCode"] == 200
        kw = mt.put_item.call_args.kwargs
        assert kw["Item"]["descriptor"] == yaml_str

    # icon is a base64 png that the desktop dashboard shows next to the device
    def test_stores_optional_icon(self, handler_module):
        mt = handler_module._mock_table
        body = {"protocol": "canbus", "icon": "base64data=="}

        result = handler_module.put_device("0x0002", body)

        assert result["statusCode"] == 200
        kw = mt.put_item.call_args.kwargs
        assert kw["Item"]["icon"] == "base64data=="

    # when protocol is omitted it should default to "unknown"
    def test_defaults_protocol_to_unknown(self, handler_module):
        mt = handler_module._mock_table

        result = handler_module.put_device("0x0003", {})

        assert result["statusCode"] == 200
        kw = mt.put_item.call_args.kwargs
        assert kw["Item"]["protocol"] == "unknown"

class TestDeleteDevice:

    # confirms the dynamodb delete_item call uses the correct key
    def test_removes_device(self, handler_module):
        mt = handler_module._mock_table

        result = handler_module.delete_device("0x0001")

        assert result["statusCode"] == 200
        mt.delete_item.assert_called_once_with(Key={"device_id": "0x0001"})

class TestGetDescriptor:

    # the gateway expects raw yaml from this endpoint, not json wrapped
    def test_returns_yaml_descriptor(self, handler_module):
        mt = handler_module._mock_table
        yaml_str = "services:\n  - name: temp\n"
        mt.get_item.return_value = {
            "Item": {"device_id": "0x0007", "descriptor": yaml_str}
        }

        result = handler_module.get_descriptor("0x0007")

        assert result["statusCode"] == 200
        assert result["headers"]["Content-Type"] == "application/x-yaml"
        assert result["body"] == yaml_str

    # device doesn't exist at all in the registry table
    def test_returns_404_when_device_not_found(self, handler_module):
        mt = handler_module._mock_table
        mt.get_item.return_value = {}

        result = handler_module.get_descriptor("0xFFFF")

        assert result["statusCode"] == 404

    # device exists but nobody uploaded a descriptor yaml yet
    def test_returns_404_when_no_descriptor(self, handler_module):
        mt = handler_module._mock_table
        mt.get_item.return_value = {
            "Item": {"device_id": "0x0007", "protocol": "modbus"}
        }

        result = handler_module.get_descriptor("0x0007")

        assert result["statusCode"] == 404
        assert "No descriptor" in result["body"]
