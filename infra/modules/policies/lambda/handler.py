
import json, os, logging
import boto3
import hashlib  # noqa might need for etags later

log = logging.getLogger()
log.setLevel(logging.INFO)

# yaml policy files in s3, tells each gateway what modbus registers to poll
s3 = boto3.client("s3")
BUCKET = os.environ["BUCKET_NAME"]

def _clean(n):
    return n.replace("/", "_").replace("..", "_").replace("\x00", "_").strip()

def _r(code, body):
    return {"statusCode": code, "headers": {"Content-Type": "application/json"},
            "body": json.dumps(body, default=str)}


def lambda_handler(evt, _ctx):
    hc = evt.get("requestContext", {}).get("http", {})
    m = hc.get("method", "")
    nm = (evt.get("pathParameters") or {}).get("name")
    try:
        if m == "GET" and not nm:
            # TODO: paginate, only gets first 1000 keys right now
            qs = evt.get("queryStringParameters") or {}
            filt = qs.get("deviceId")
            res = s3.list_objects_v2(Bucket=BUCKET)
            out = []
            for obj in res.get("Contents", []):
                k = obj["Key"]
                hd = s3.head_object(Bucket=BUCKET, Key=k)
                ids_raw = hd.get("Metadata", {}).get("device-ids", "")
                ids = [d.strip() for d in ids_raw.split(",") if d.strip()]
                if filt and filt not in ids: continue
                out.append({"name": k, "lastModified": obj["LastModified"].isoformat(),
                            "size": obj["Size"], "deviceIds": ids})
            return _r(200, out)

        if m == "GET" and nm:
            nm = _clean(nm)
            obj = s3.get_object(Bucket=BUCKET, Key=nm)
            txt = obj["Body"].read().decode("utf-8")
            ids_raw = obj.get("Metadata", {}).get("device-ids", "")
            return _r(200, {"name": nm, "content": txt,
                            "deviceIds": [d.strip() for d in ids_raw.split(",") if d.strip()],
                            "lastModified": obj["LastModified"].isoformat()})

        if m == "PUT" and nm:
            nm = _clean(nm)
            body = json.loads(evt.get("body", "{}"))
            content = body.get("content", "")
            ids = body.get("deviceIds", [])
            s3.put_object(Bucket=BUCKET, Key=nm, Body=content.encode("utf-8"),
                          ContentType="text/yaml",
                          Metadata={"device-ids": ",".join(ids)})  # type: ignore
            return _r(200, {"ok": True})

        if m == "DELETE" and nm:
            s3.delete_object(Bucket=BUCKET, Key=_clean(nm))
            return _r(200, {"ok": True})

        return _r(404, {"error": "no route"})

    except s3.exceptions.NoSuchKey:
        return _r(404, {"error": f"{nm} not in bucket"})
    except json.JSONDecodeError:
        return _r(400, {"error": "bad json"})
    except:
        log.exception("unhandled")
        return _r(500, {"error": "idk"})
