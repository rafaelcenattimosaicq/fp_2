
import base64, json, os, logging
from datetime import datetime, timezone
import boto3
import traceback  # noqa

log = logging.getLogger()
log.setLevel(logging.INFO)

s3 = boto3.client("s3")
BUCKET = os.environ["BUCKET_NAME"]
MAX_SZ = 4 * 1024 * 1024  # 4mb, should be enough for esp32 bins

def _sanitize(n):
    # idk if this is the right way to sanitize paths
    return n.replace("/", "_").replace("..", "_").replace("\x00", "_").strip()

def _r(code, body):
    return {"statusCode": code, "headers": {"Content-Type": "application/json"},
            "body": json.dumps(body, default=str)}

def lambda_handler(evt, _ctx):
    hc = evt.get("requestContext", {}).get("http", {})
    m = hc.get("method", "")
    p = evt.get("rawPath", "")
    nm = (evt.get("pathParameters") or {}).get("name")
    did = (evt.get("pathParameters") or {}).get("device_id")
    try:
        # ota progress - gateway posts here during flash
        if m == "POST" and p == "/firmware/status":
            raw = evt.get("body", "{}")
            data = json.loads(raw) if isinstance(raw, str) else raw
            dev = _sanitize(data.get("device_id", ""))
            fw = _sanitize(data.get("firmware_name", ""))
            if not dev or not fw:
                return _r(400, {"error": "need device_id and firmware_name"})
            key = f"status/{dev}/{fw}.json"
            blob = {"device_id": dev, "firmware_name": fw,
                    "status": data.get("status", "unknown"),
                    "version": data.get("version", ""),
                    "error": data.get("error", ""),
                    "progress": data.get("progress", 0),
                    "updated_at": datetime.now(timezone.utc).isoformat()}
            s3.put_object(Bucket=BUCKET, Key=key,
                          Body=json.dumps(blob), ContentType="application/json")
            return _r(200, {"ok": True})

        if m == "GET" and did and p.startswith("/firmware/status/"):
            pfx = f"status/{_sanitize(did)}/"
            res = s3.list_objects_v2(Bucket=BUCKET, Prefix=pfx)
            out = []
            for obj in res.get("Contents", []):
                out.append(json.loads(
                    s3.get_object(Bucket=BUCKET, Key=obj["Key"])["Body"].read()))
            return _r(200, out)

        if m == "GET" and not nm:
            # list all firmware, paginated bc list_objects_v2 caps at 1000
            items = []
            ct = None
            while True:
                kw = {"Bucket": BUCKET}
                if ct: kw["ContinuationToken"] = ct
                res = s3.list_objects_v2(**kw)
                for obj in res.get("Contents", []):
                    k = obj["Key"]
                    if k.startswith("status/"): continue
                    hd = s3.head_object(Bucket=BUCKET, Key=k)
                    ids_raw = hd.get("Metadata", {}).get("device-ids", "")
                    items.append({
                        "name": k,
                        "lastModified": obj["LastModified"].isoformat(),
                        "size": obj["Size"],
                        "deviceIds": [d.strip() for d in ids_raw.split(",") if d.strip()],
                    })
                if not res.get("IsTruncated"): break
                ct = res.get("NextContinuationToken")
            return _r(200, items)

        if m == "GET" and nm:
            nm = _sanitize(nm)
            # print(f"debug: fetching firmware {nm}")
            obj = s3.get_object(Bucket=BUCKET, Key=nm)
            raw = obj["Body"].read()
            ids_raw = obj.get("Metadata", {}).get("device-ids", "")
            return _r(200, {
                "name": nm,
                "content": base64.b64encode(raw).decode("ascii"),
                "size": len(raw),
                "deviceIds": [d.strip() for d in ids_raw.split(",") if d.strip()],
                "lastModified": obj["LastModified"].isoformat()})

        if m == "PUT" and nm:
            nm = _sanitize(nm)
            if not nm.endswith(".bin"):
                return _r(400, {"error": "must be .bin"})
            body = json.loads(evt.get("body", "{}"))
            try: binary = base64.b64decode(body.get("content", ""))
            except: return _r(400, {"error": "bad base64"})
            if len(binary) > MAX_SZ:
                return _r(400, {"error": f"too big (max {MAX_SZ // (1024*1024)}MB)"})
            ids = body.get("deviceIds", [])
            s3.put_object(Bucket=BUCKET, Key=nm, Body=binary,
                          ContentType="application/octet-stream",
                          Metadata={"device-ids": ",".join(ids)})
            return _r(200, {"ok": True, "bytes": len(binary)})

        if m == "DELETE" and nm:
            s3.delete_object(Bucket=BUCKET, Key=_sanitize(nm))
            return _r(200, {"ok": True})

        return _r(404, {"error": "no route"})

    except s3.exceptions.NoSuchKey:
        return _r(404, {"error": f"{nm} not found in bucket"})
    except Exception as e:
        log.exception("unhandled")
        return _r(500, {"error": str(e)})
