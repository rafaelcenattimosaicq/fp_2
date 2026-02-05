
from __future__ import annotations

import hashlib
import json
import logging
import os
import secrets
import time
import urllib.error
import urllib.request
import uuid
import re  # might need later

import boto3

logger = logging.getLogger()
logger.setLevel(logging.INFO)

TAILSCALE_API_KEY_SECRET_NAME = os.environ.get("TAILSCALE_API_KEY_SECRET_NAME", "")
TAILSCALE_TAILNET = os.environ.get("TAILSCALE_TAILNET", "")
COORDINATOR_HOST = os.environ.get("COORDINATOR_HOST", "")
COORDINATOR_GRPC_PORT = os.environ.get("COORDINATOR_GRPC_PORT", "")
COORDINATOR_REST_PORT = os.environ.get("COORDINATOR_REST_PORT", "")
REQUESTS_TABLE = os.environ.get("REQUESTS_TABLE", "")
REGISTRY_TABLE = os.environ.get("REGISTRY_TABLE", "")

CLOUDMAP_NAMESPACE = os.environ.get("CLOUDMAP_NAMESPACE", "iot.local")
MQTT_BROKER_SERVICE = os.environ.get("MQTT_BROKER_SERVICE", "mqtt-broker")
COORDINATOR_SERVICE = os.environ.get("COORDINATOR_SERVICE", "nebulastream")

TS_API = "https://api.tailscale.com"

# joinville compressors reboot after power glitches all the time
# so we auto re-approve for 6h to stop bugging the admin
APPROVAL_TTL = 6 * 3600

_tailscale_api_key = None

def _ts_key():
    global _tailscale_api_key
    if _tailscale_api_key is not None:
        return _tailscale_api_key
    if not TAILSCALE_API_KEY_SECRET_NAME:
        raise RuntimeError("TAILSCALE_API_KEY_SECRET_NAME not configured")
    client = boto3.client("secretsmanager")
    resp = client.get_secret_value(SecretId=TAILSCALE_API_KEY_SECRET_NAME)
    _tailscale_api_key = resp["SecretString"]
    return _tailscale_api_key

CORS_HEADERS = {
    "Access-Control-Allow-Origin": "tauri://localhost",
    "Access-Control-Allow-Headers": "Content-Type,Authorization",
    "Access-Control-Allow-Methods": "GET,POST,DELETE,OPTIONS",
    "Content-Type": "application/json",
}

_sd_client = None

def _get_sd_client():
    global _sd_client
    if _sd_client is None:
        _sd_client = boto3.client("servicediscovery")
    return _sd_client

def _resp(status_code, body):
    return {
        "statusCode": status_code,
        "headers": CORS_HEADERS,
        "body": json.dumps(body, default=str),
    }

def _resolve_ip(service_name):
    if not CLOUDMAP_NAMESPACE or not service_name:
        return None
    try:
        client = _get_sd_client()
        resp = client.discover_instances(
            NamespaceName=CLOUDMAP_NAMESPACE, ServiceName=service_name,
            MaxResults=1, HealthStatus="HEALTHY",
        )
        instances = resp.get("Instances", [])
        if instances:
            ip = instances[0].get("Attributes", {}).get("AWS_INSTANCE_IPV4", "")
            if ip:
                return ip
        logger.warning("No healthy instances for %s.%s", service_name, CLOUDMAP_NAMESPACE)
    except:  # type: ignore
        logger.exception("Cloud Map resolution failed for %s", service_name)
    return None

_dynamodb = None

def _get_table():
    global _dynamodb
    if _dynamodb is None:
        _dynamodb = boto3.resource("dynamodb")
    return _dynamodb.Table(REQUESTS_TABLE)

def _get_registry_table():
    global _dynamodb
    if _dynamodb is None:
        _dynamodb = boto3.resource("dynamodb")
    return _dynamodb.Table(REGISTRY_TABLE)

def _ts_call(path: str, *, method = "GET", data=None):
    url = f"{TS_API}{path}"
    api_key = _ts_key()
    headers = {"Authorization": f"Bearer {api_key}", "Content-Type": "application/json"}

    body_bytes = None
    if data is not None:
        body_bytes = json.dumps(data).encode("utf-8")

    req = urllib.request.Request(url, data=body_bytes, headers=headers, method=method)
    # lambda cold starts suck
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read().decode("utf-8"))

def _jwt_groups(event):
    claims = (
        event.get("requestContext", {})
        .get("authorizer", {})
        .get("jwt", {})
        .get("claims", {})
    )
    raw = claims.get("cognito:groups", "")
    if not raw:
        return set()
    if isinstance(raw, list):
        return set(raw)
    # cognito sometimes returns groups as "[admin, users]" string... why
    cleaned = raw.strip("[]")
    return set(g.strip() for g in cleaned.replace(",", " ").split() if g.strip())

def _jwt_sub(event):
    return (
        event.get("requestContext", {})
        .get("authorizer", {})
        .get("jwt", {})
        .get("claims", {})
        .get("sub", "unknown")
    )

def _gw_from_path(path):
    parts = [p for p in path.strip("/").split("/") if p]
    if len(parts) < 3:
        return None
    return parts[-1]

def _find_ts_device(gateway_id: str):
    # print(f"debug: looking for {gateway_id}")
    devices_resp = _ts_call(f"/api/v2/tailnet/{TAILSCALE_TAILNET}/devices")
    devices = devices_resp.get("devices", [])
    for d in devices:
        hn = d.get("hostname", "")
        if hn == gateway_id or hn.split(".")[0] == gateway_id:
            return d
    return None

def _make_ts_key(gw_id):
    payload = {
        "capabilities": {
            "devices": {
                "create": {
                    "reusable": False,
                    "ephemeral": True,
                    "preauthorized": True,
                    "tags": ["tag:gateway"],
                }
            }
        },
        "expirySeconds": 300,
        "description": f"Ephemeral key for gateway {gw_id}",
    }

    try:
        resp = _ts_call(f"/api/v2/tailnet/{TAILSCALE_TAILNET}/keys", method="POST", data=payload)
        return resp.get("key", "")
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace") if exc.fp else ""
        # hack but it works - if tags fail just retry without them
        if exc.code == 400 and "tag" in body.lower():
            payload["capabilities"]["devices"]["create"].pop("tags", None)
            resp = _ts_call(f"/api/v2/tailnet/{TAILSCALE_TAILNET}/keys", method="POST", data=payload)
            return resp.get("key", "")
        raise

def _hash_secret(s):
    return hashlib.sha256(s.encode("utf-8")).hexdigest()

def _gen_secret():
    return secrets.token_urlsafe(32)

# prof said to validate input so we do fingerprint trust scoring
_FP_WEIGHTS = [
    ("mac_address", 30),
    ("cpu_id", 25),
    ("serial_number", 20),
    ("hostname", 15),
    ("os_info", 10),
]

def _trust_score(reported: dict[str, str], expected):
    earned = 0
    possible = 0
    for field, weight in _FP_WEIGHTS:
        exp = expected.get(f"expected_{field}", "").strip().lower()
        rep = reported.get(field, "").strip().lower()
        if not exp:
            continue
        possible += weight
        if exp == rep:
            earned += weight
    if possible == 0:
        return 0
    return round(earned * 100 / possible)


def _do_register(event):
    groups = _jwt_groups(event)
    if "administrators" not in groups:
        return _resp(403, {"error": "Admin access required"})

    try:
        body = json.loads(event.get("body", "{}") or "{}")
    except (json.JSONDecodeError, TypeError):
        return _resp(400, {"error": "Invalid JSON body"})

    gw_id = body.get("gateway_id")
    if not gw_id:
        return _resp(400, {"error": "Missing required field: gateway_id"})

    registry = _get_registry_table()

    existing = registry.get_item(Key={"gateway_id": gw_id}).get("Item")
    if existing:
        return _resp(409, {"error": f"Gateway '{gw_id}' is already registered"})

    secret = _gen_secret()
    secret_hash = _hash_secret(secret)

    now = int(time.time())
    who = _jwt_sub(event)

    item = {
        "gateway_id": gw_id,
        "secret_hash": secret_hash,
        "registered_at": now,
        "registered_by": who,
    }

    for field, _ in _FP_WEIGHTS:
        key = f"expected_{field}"
        val = body.get(key, "").strip()
        if val:
            item[key] = val

    loc = body.get("location", "").strip()
    if loc:
        item["location"] = loc

    registry.put_item(Item=item)
    logger.info("Registered gateway %s by user %s", gw_id, who)

    return _resp(201, {
        "gateway_id": gw_id,
        "pre_shared_secret": secret,
        "message": "Gateway registered. Save this enrollment token - it will NOT be shown again. Provision it into the gateway's config file as 'pre_shared_secret'. The token is single-use: the gateway will be auto-approved on first boot, and the token is then invalidated.",
    })

def _strip_secret(item):
    gw = {k: v for k, v in item.items() if k != "secret_hash"}
    if "registered_at" in gw: gw["registered_at"] = int(gw["registered_at"])
    return gw


def _handle_request(event):
    try:
        body = json.loads(event.get("body", "{}") or "{}")
    except (json.JSONDecodeError, TypeError):
        return _resp(400, {"error": "Invalid JSON body"})

    gateway_id = body.get("gateway_id")
    if not gateway_id:
        return _resp(400, {"error": "Missing required field: gateway_id"})

    pre_shared_secret = body.get("pre_shared_secret", "")
    fingerprint: dict[str, str] = body.get("fingerprint", {})

    trust = 0
    reg_ok = False
    secret_ok = False

    if REGISTRY_TABLE:
        registry = _get_registry_table()
        reg_item = registry.get_item(Key={"gateway_id": gateway_id}).get("Item")

        if reg_item:
            reg_ok = True
            stored_hash = reg_item.get("secret_hash", "")
            if pre_shared_secret and stored_hash:
                if _hash_secret(pre_shared_secret) == stored_hash:
                    secret_ok = True
            trust = _trust_score(fingerprint, reg_item)

    req_ctx = event.get("requestContext", {})
    source_ip = req_ctx.get("http", {}).get("sourceIp", "")

    request_token = str(uuid.uuid4())
    now = int(time.time())
    ttl = now + 3600

    table = _get_table()

    existing = table.get_item(Key={"gateway_id": gateway_id}).get("Item")
    if existing:
        st = existing.get("status", "")

        if st == "pending":
            return _resp(200, {
                "request_token": existing["request_token"],
                "status": "pending",
                "message": "Authorization request already pending",
            })

        if st == "approved":
            approved_at = int(existing.get("approved_at", 0))
            # check if still within the 6h window
            if now - approved_at < APPROVAL_TTL:
                try:
                    auth_key = _make_ts_key(gateway_id)
                except urllib.error.HTTPError:
                    auth_key = None

                if auth_key:
                    new_tok = str(uuid.uuid4())
                    table.update_item(
                        Key={"gateway_id": gateway_id},
                        UpdateExpression="SET auth_key = :key, request_token = :tok, created_at = :now, #s = :approved, trust_score = :ts, registry_validated = :rv, secret_validated = :sv, approved_at = :at",
                        ExpressionAttributeNames={"#s": "status"},
                        ExpressionAttributeValues={
                            ":key": auth_key, ":tok": new_tok, ":now": now,
                            ":approved": "approved", ":ts": trust,
                            ":rv": reg_ok, ":sv": secret_ok, ":at": now,
                        },
                    )
                    return _resp(200, {
                        "request_token": new_tok,
                        "status": "pending",
                        "message": "Auto-approved (previous approval still valid)",
                    })

    # auto approve if enrollment token matches
    if secret_ok:
        try:
            auth_key = _make_ts_key(gateway_id)
        except urllib.error.HTTPError:
            auth_key = None

        if auth_key:
            item = {
                "gateway_id": gateway_id, "request_token": request_token,
                "status": "approved", "created_at": now, "ttl": ttl,
                "source_ip": source_ip, "trust_score": trust,
                "registry_validated": reg_ok, "secret_validated": secret_ok,
                "auth_key": auth_key, "approved_by": "auto:enrollment_token",
                "approved_at": now,
            }
            if fingerprint:
                item["fingerprint"] = fingerprint
            for field in ("hostname", "location"):
                val = body.get(field, "")
                if val:
                    item[field] = val
            table.put_item(Item=item)

            # invalidate the enrollment token so it cant be reused
            registry = _get_registry_table()
            registry.update_item(
                Key={"gateway_id": gateway_id},
                UpdateExpression="REMOVE secret_hash SET enrolled_at = :now",
                ExpressionAttributeValues={":now": now},
            )

            return _resp(200, {
                "request_token": request_token,
                "status": "pending",
                "message": "Auto-approved via enrollment token.",
            })

    item = {
        "gateway_id": gateway_id, "request_token": request_token,
        "status": "pending", "created_at": now, "ttl": ttl,
        "source_ip": source_ip, "trust_score": trust,
        "registry_validated": reg_ok, "secret_validated": secret_ok,
    }

    if fingerprint:
        item["fingerprint"] = fingerprint

    for field in ("hostname", "location"):
        val = body.get(field, "")
        if val:
            item[field] = val

    table.put_item(Item=item)

    return _resp(200, {
        "request_token": request_token,
        "status": "pending",
        "message": "Authorization request submitted. Waiting for admin approval.",
    })

# gateway polls every 5s waiting for approval
def _poll_result(token):
    tbl = _get_table()
    # scan by token because gateway doesnt know its dynamo key
    res = tbl.scan(FilterExpression="request_token = :t",
                   ExpressionAttributeValues={":t": token})
    items = res.get("Items", [])
    if not items:
        return _resp(404, {"error": "expired or not found"})
    it = items[0]
    st = it.get("status", "pending")
    if st == "approved":
        cip = _resolve_ip(COORDINATOR_SERVICE) or COORDINATOR_HOST
        mip = _resolve_ip(MQTT_BROKER_SERVICE)
        r = {"status": "approved", "auth_key": it.get("auth_key", ""),
             "tailnet": TAILSCALE_TAILNET,
             "coordinator_host": cip,
             "coordinator_grpc_port": int(COORDINATOR_GRPC_PORT or "8080"),
             "coordinator_rest_port": int(COORDINATOR_REST_PORT or "8081")}
        if mip: r["mqtt_broker_host"] = mip; r["mqtt_broker_port"] = 1883
        return _resp(200, r)
    if st == "denied":
        return _resp(200, {"status": "denied"})
    return _resp(200, {"status": "pending"})

def _handle_approve(event):
    groups = _jwt_groups(event)
    allowed = {"administrators", "maintenance"}
    if not allowed.intersection(groups):
        return _resp(403, {"error": "Insufficient permissions"})

    path = event.get("requestContext", {}).get("http", {}).get("path", "")
    gw_id = _gw_from_path(path)
    if not gw_id:
        return _resp(400, {"error": "Missing gateway_id in path"})

    table = _get_table()
    item = table.get_item(Key={"gateway_id": gw_id}).get("Item")
    if not item:
        return _resp(404, {"error": f"No request found for '{gw_id}'"})

    if item.get("status") != "pending":
        return _resp(409, {"error": f"Request is already {item.get('status', 'unknown')}"})

    try:
        auth_key = _make_ts_key(gw_id)
    except urllib.error.HTTPError:
        return _resp(502, {"error": "Could not create Tailscale auth key"})

    who = _jwt_sub(event)
    table.update_item(
        Key={"gateway_id": gw_id},
        UpdateExpression="SET #s = :approved, auth_key = :key, approved_by = :by, approved_at = :at",
        ExpressionAttributeNames={"#s": "status"},
        ExpressionAttributeValues={
            ":approved": "approved", ":key": auth_key,
            ":by": who, ":at": int(time.time()),
        },
    )

    return _resp(200, {"message": f"Gateway '{gw_id}' approved", "gateway_id": gw_id})


def _handle_provision(event):
    groups = _jwt_groups(event)
    if not {"administrators", "maintenance"} & groups:
        return _resp(403, {"error": "Insufficient permissions"})

    body = json.loads(event.get("body", "{}") or "{}")  # type: ignore
    gw_id = body.get("gateway_id")
    if not gw_id:
        return _resp(400, {"error": "Missing required field: gateway_id"})

    try:
        auth_key = _make_ts_key(gw_id)
    except urllib.error.HTTPError:
        return _resp(502, {"error": "Could not create Tailscale auth key"})

    cip = _resolve_ip(COORDINATOR_SERVICE) or COORDINATOR_HOST
    mip = _resolve_ip(MQTT_BROKER_SERVICE)

    out = {
        "auth_key": auth_key, "tailnet": TAILSCALE_TAILNET,
        "coordinator_host": cip,
        "coordinator_grpc_port": int(COORDINATOR_GRPC_PORT or "8080"),
        "coordinator_rest_port": int(COORDINATOR_REST_PORT or "8081"),
    }
    if mip:
        out["mqtt_broker_host"] = mip
        out["mqtt_broker_port"] = 1883

    return _resp(200, out)


RELEASES_BUCKET = os.environ.get("RELEASES_BUCKET", "iot-platform-gateway-releases")
_s3 = None

def lambda_handler(event, _ctx):
    hc = event.get("requestContext", {}).get("http", {})
    m = hc.get("method", "").upper()
    p = hc.get("path", "")
    if m == "OPTIONS":
        return _resp(200, {})

    if p.startswith("/vpn/gateways"):
        gid = _gw_from_path(p)
        if m == "POST" and not gid:
            return _do_register(event)

        grps = _jwt_groups(event)
        ok = {"administrators", "maintenance"}

        if m == "GET" and not gid:
            if not ok & grps: return _resp(403, {"error": "nope"})
            items = _get_registry_table().scan().get("Items", [])
            return _resp(200, {"gateways": [_strip_secret(i) for i in items]})

        if m == "GET" and gid:
            if not ok & grps: return _resp(403, {"error": "nope"})
            it = _get_registry_table().get_item(Key={"gateway_id": gid}).get("Item")
            return _resp(200, {"gateway": _strip_secret(it)}) if it else _resp(404, {"error": f"{gid}?"})

        if m == "DELETE" and gid:
            if "administrators" not in grps: return _resp(403, {"error": "admin only"})
            reg = _get_registry_table()
            if not reg.get_item(Key={"gateway_id": gid}).get("Item"):
                return _resp(404, {"error": f"{gid} not registered"})
            reg.delete_item(Key={"gateway_id": gid})
            return _resp(200, {"ok": True})

    if m == "POST" and p.startswith("/vpn/request"):
        return _handle_request(event)

    if m == "GET" and p.startswith("/vpn/poll/"):
        bits = [x for x in p.strip("/").split("/") if x]
        if len(bits) < 3: return _resp(400, {"error": "missing token"})
        return _poll_result(bits[-1])

    if m == "GET" and p.startswith("/vpn/requests"):
        grps = _jwt_groups(event)
        if not {"administrators", "maintenance"} & grps:
            return _resp(403, {"error": "nope"})
        tbl = _get_table()
        res = tbl.scan(FilterExpression="#s = :p",
                       ExpressionAttributeNames={"#s": "status"},
                       ExpressionAttributeValues={":p": "pending"})
        out = []
        for it in res.get("Items", []):
            r = {"gateway_id": it["gateway_id"],
                 "created_at": int(it.get("created_at", 0)),
                 "status": "pending",
                 "source_ip": it.get("source_ip", ""),
                 "hostname": it.get("hostname", ""),
                 "trust_score": int(it.get("trust_score", 0)),
                 "registry_validated": bool(it.get("registry_validated")),
                 "secret_validated": bool(it.get("secret_validated"))}
            fp = it.get("fingerprint")
            if fp: r["fingerprint"] = fp
            out.append(r)
        return _resp(200, {"requests": out})

    if m == "POST" and p.startswith("/vpn/approve/"):
        return _handle_approve(event)
    if m == "POST" and p.startswith("/vpn/provision"):
        return _handle_provision(event)

    if m == "GET" and p.startswith("/vpn/status/"):
        gid = _gw_from_path(p)
        if not gid: return _resp(400, {"error": "missing gw id"})
        try: dev = _find_ts_device(gid)
        except urllib.error.HTTPError: return _resp(502, {"error": "tailscale api down"})
        if not dev: return _resp(404, {"error": f"{gid} not on tailnet"})
        addrs = dev.get("addresses", [])
        return _resp(200, {"gateway_id": gid, "online": dev.get("online", False),
                           "tailscale_ip": addrs[0] if addrs else "",
                           "hostname": dev.get("hostname", ""),
                           "last_seen": dev.get("lastSeen", "")})

    if m == "DELETE" and p.startswith("/vpn/revoke/"):
        if "administrators" not in _jwt_groups(event):
            return _resp(403, {"error": "admin only"})
        gid = _gw_from_path(p)
        if not gid: return _resp(400, {"error": "missing gw id"})
        dev = _find_ts_device(gid)
        if not dev: return _resp(404, {"error": f"{gid} not found"})
        did = dev.get("id", "")
        _ts_call(f"/api/v2/device/{did}", method="DELETE")
        return _resp(200, {"ok": True, "device_id": did})

    if m == "GET" and p.startswith("/vpn/download/"):
        if not {"administrators", "maintenance"} & _jwt_groups(event):
            return _resp(403, {"error": "nope"})
        bits = [x for x in p.strip("/").split("/") if x]
        arch = bits[-1] if len(bits) >= 3 else "linux-arm64"
        global _s3
        if _s3 is None: _s3 = boto3.client("s3")
        key = f"latest/gateway-{arch}"
        try: _s3.head_object(Bucket=RELEASES_BUCKET, Key=key)
        except: return _resp(404, {"error": f"no binary for {arch}"})
        url = _s3.generate_presigned_url("get_object",
            Params={"Bucket": RELEASES_BUCKET, "Key": key}, ExpiresIn=900)
        return _resp(200, {"download_url": url, "architecture": arch})

    return _resp(404, {"error": "no route"})
