
const EMBRACO_HOLDING_MIN: u16 = 0x0000;
const EMBRACO_HOLDING_MAX: u16 = 0x00FF;
const EMBRACO_INPUT_MIN: u16 = 0x1000;
const EMBRACO_INPUT_MAX: u16 = 0x10FF;
const TURBINE_RANGE_START: u16 = 0x2000;

const REGISTER_TYPES: &[&str] = &["holding", "input", "coil", "discrete"];


const MIN_INTERVAL_MS: u32 = 100;
const MAX_INTERVAL_MS: u32 = 60_000;

const RESERVED_PREFIXES: &[&str] = &["status/", "_internal/"];
const MAX_NAME_LEN: usize = 128;


const S3_META_CAP: usize = 2048;

fn parece_yaml(raw: &str) -> bool {
    let l = raw.trim_start();
    if l.is_empty() { return false; }
    match l.as_bytes()[0] {
        b'#' | b'-' => true,
        _ => l.starts_with("---") || l.contains(": "),
    }
}

fn check_addr(addr: u16) -> Option<&'static str> {
    if addr >= EMBRACO_HOLDING_MIN && addr <= EMBRACO_HOLDING_MAX { return None; }
    if addr >= EMBRACO_INPUT_MIN && addr <= EMBRACO_INPUT_MAX { return None; }
    if addr >= TURBINE_RANGE_START { return None; } 
    Some("address outside known register ranges")
}

use aws_sdk_s3::Client as S3Client;
use lambda_http::{run, service_fn, Body, Error, Request, RequestExt, Response};
use serde_json::{json, Value};
use std::env;
use tracing::{error, warn};

async fn handler(event: Request, s3: &S3Client, bucket: &str) -> Result<Response<Body>, Error> {
    let method = event.method().as_str().to_uppercase();
    let nm: Option<String> = event.path_parameters().first("name").map(|s| s.to_string());

    let ok = |code: u16, body: Value| -> Response<Body> {
        Response::builder().status(code)
            .header("Content-Type", "application/json")
            .body(Body::Text(body.to_string())).unwrap()
    };

    let result: Result<Response<Body>, Error> = async {
  
        if method == "GET" && nm.is_none() {
            let qs = event.query_string_parameters();
            let filt = qs.first("deviceId").map(|s| s.to_string());
            let list = s3.list_objects_v2().bucket(bucket).send().await?;
            let mut out: Vec<Value> = Vec::new();
            for obj in list.contents() {
                let key = match obj.key() { Some(k) => k, None => continue };
                let head = s3.head_object().bucket(bucket).key(key).send().await?;
                let raw = head.metadata().and_then(|m| m.get("device-ids")).map(|s| s.as_str()).unwrap_or("");
                let ids: Vec<&str> = raw.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                if let Some(ref f) = filt {
                    if !ids.contains(&f.as_str()) { continue; }
                }
                let lm = obj.last_modified()
                    .and_then(|t| t.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime).ok())
                    .unwrap_or_default();
                out.push(json!({"name": key, "lastModified": lm, "size": obj.size().unwrap_or(0), "deviceIds": ids}));
            }
            return Ok(ok(200, Value::Array(out)));
        }

        // GET /policies/{name}
        if method == "GET" && nm.is_some() {
            let name = nm.as_ref().unwrap().replace('/', "_").replace("..", "_").trim().to_string();
            match s3.get_object().bucket(bucket).key(&name).send().await {
                Ok(obj) => {
                    let ids_csv = obj.metadata().and_then(|m| m.get("device-ids")).cloned().unwrap_or_default();
                    let ids: Vec<&str> = ids_csv.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                    let lm = obj.last_modified()
                        .and_then(|t| t.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime).ok())
                        .unwrap_or_default();
                    let bytes = obj.body.collect().await?.into_bytes();
                    let txt = String::from_utf8_lossy(&bytes);
                    return Ok(ok(200, json!({"name": name, "content": txt, "deviceIds": ids, "lastModified": lm})));
                }
                Err(e) => {
                    let svc = e.into_service_error();
                    if svc.is_no_such_key() { return Ok(ok(404, json!({"error": format!("{name} not in bucket")}))); }
                    return Err(Error::from(svc));
                }
            }
        }

        // PUT /policies/{name}
        if method == "PUT" && nm.is_some() {
            let name = nm.as_ref().unwrap().replace('/', "_").replace("..", "_").trim().to_string();
            if name.is_empty() { return Ok(ok(400, json!({"error": "empty policy name"}))); }
            if name.len() > MAX_NAME_LEN { return Ok(ok(400, json!({"error": format!("name too long ({} > {})", name.len(), MAX_NAME_LEN)}))); }
            for pfx in RESERVED_PREFIXES {
                if name.starts_with(pfx) { return Ok(ok(400, json!({"error": format!("name cannot start with '{pfx}'")}))); }
            }

            let body_str = match event.body() {
                Body::Text(t) => t.clone(),
                Body::Binary(b) => String::from_utf8_lossy(b).to_string(),
                Body::Empty => "{}".into(),
            };
            let data: Value = match serde_json::from_str(&body_str) {
                Ok(v) => v,
                Err(_) => return Ok(ok(400, json!({"error": "bad json"}))),
            };

            let content = data.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if !content.is_empty() && !parece_yaml(&content) {
                warn!(name = %name, "content doesn't look like YAML");
            }

            let ids: Vec<String> = data.get("deviceIds").and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let csv = ids.join(",");
            if csv.len() > S3_META_CAP {
                warn!(name = %name, n = ids.len(), "device-ids metadata may exceed S3 2KB cap");
            }

            s3.put_object().bucket(bucket).key(&name)
                .body(aws_sdk_s3::primitives::ByteStream::from(content.into_bytes()))
                .content_type("text/yaml")
                .metadata("device-ids", &csv)
                .send().await?;
            return Ok(ok(200, json!({"ok": true})));
        }

        // DELETE /policies/{name}
        if method == "DELETE" && nm.is_some() {
            let name = nm.as_ref().unwrap().replace('/', "_").replace("..", "_");
            s3.delete_object().bucket(bucket).key(&name).send().await?;
            return Ok(ok(200, json!({"ok": true})));
        }

        Ok(ok(404, json!({"error": "no route"})))
    }.await;

    match result {
        Ok(r) => Ok(r),
        Err(e) => { error!(error = %e, "unhandled"); Ok(ok(500, json!({"error": e.to_string()}))) }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .without_time().init();
    let cfg = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let s3 = S3Client::new(&cfg);
    let bucket = env::var("BUCKET_NAME").expect("BUCKET_NAME required");
    run(service_fn(|req| handler(req, &s3, &bucket))).await
}
