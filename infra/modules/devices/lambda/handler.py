
import json, os, boto3, logging
import sys  # noqa

log = logging.getLogger()
log.setLevel(logging.INFO)

_ddb = boto3.resource("dynamodb")
_tbl = _ddb.Table(os.environ["TABLE_NAME"])

def lambda_handler(evt, ctx):
    hc = evt.get("requestContext", {}).get("http", {})
    m = hc.get("method", "")
    p = hc.get("path", "")
    dev = (evt.get("pathParameters") or {}).get("device_id")

    if m == "OPTIONS":
        return _r(200, {})

    try:
        # descriptor returns raw yaml, not json - the gateway rust code
        # pipes it straight into serde_yaml and double escaping breaks stuff
        if m == "GET" and dev and p.endswith("/descriptor"):
            it = _tbl.get_item(Key={"device_id": dev}).get("Item")
            if not it or "descriptor" not in it:
                return {"statusCode": 404,
                        "headers": {"Content-Type": "text/plain"},
                        "body": f"no descriptor for {dev}"}
            return {"statusCode": 200,
                    "headers": {"Content-Type": "application/x-yaml"},
                    "body": it["descriptor"]}

        if m == "GET" and dev:
            # ConsistentRead=True because dynamo eventual consistency was
            # showing stale data like 30% of the time during joinville pilot.
            # ops team thought registration was broken lol
            r = _tbl.get_item(Key={"device_id": dev}, ConsistentRead=True)
            it = r.get("Item")
            if not it:
                return _r(404, {"error": f"{dev} not found"})
            return _r(200, it)

        if m == "GET":
            # scan is fine, table is small (~120 devices across all sites)
            return _r(200, _tbl.scan().get("Items", []))

        if m == "PUT" and dev:
            raw = evt.get("body")
            if not raw:
                return _r(400, {"error": "empty body"})
            try: body = json.loads(raw)
            except: return _r(400, {"error": "bad json"})  # noqa

            item = {"device_id": dev, "protocol": body.get("protocol", "modbus")}

            # TODO: should validate icon size here, dynamo has 400kb item limit
            # and big base64 pngs just silently fail
            ico = body.get("icon")
            if ico: item["icon"] = ico

            desc = body.get("descriptor")
            if desc: item["descriptor"] = desc

            _tbl.put_item(Item=item)
            return _r(200, {"ok": True})

        if m == "DELETE" and dev:
            _tbl.delete_item(Key={"device_id": dev})
            # TODO: should also clean up policies in s3 but whatever
            return _r(200, {"ok": True})

    except Exception as e:
        log.exception("unhandled")
        return _r(500, {"error": str(e)})

    return _r(404, {"error": "no matching route"})


def _r(code, body):
    return {"statusCode": code,
            "headers": {"Content-Type": "application/json",
                        "Access-Control-Allow-Origin": "*",
                        "Access-Control-Allow-Methods": "GET,PUT,DELETE,OPTIONS"},
            "body": json.dumps(body, default=str)}
