
const ESP32_MAGIC: u8 = 0xE9;
const ESP32_MAX_SEGMENTS: u8 = 16;

const MAX_FIRMWARE_BYTES: usize = 4 * 1024 * 1024;

const _NAMING_RE: &str = r"^[a-z0-9]+-[a-z0-9]+-v\d+\.\d+\.\d+";

fn check_esp32_header(raw: &[u8]) -> Option<String> {
    if raw.is_empty() {
        return Some("empty binary".into());
    }
    if raw[0] != ESP32_MAGIC {
        let hint = if raw[0] == 0x7F { " (looks like an ELF file, need .bin)" } else { "" };
        return Some(format!(
            "bad magic byte: expected 0xE9, got 0x{:02X}{hint}", raw[0]
        ));
    }
    if raw.len() > 1 && raw[1] > ESP32_MAX_SEGMENTS {
        return Some(format!(
            "suspicious segment count {} (max {}), binary might be truncated",
            raw[1], ESP32_MAX_SEGMENTS
        ));
    }
    None
}

use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::primitives::ByteStream;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use lambda_http::{
    http::StatusCode, run, service_fn, Body, Error, Request, RequestExt, Response,
};
use serde_json::{json, Value};
use std::env;
use tracing::{error, info, warn};


macro_rules! cors_json {
    ($code:expr, $body:expr) => {
        Response::builder()
            .status($code)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .header("Access-Control-Allow-Methods", "GET,POST,PUT,DELETE,OPTIONS")
            .header("Access-Control-Allow-Headers", "Content-Type,Authorization")
            .body(Body::from(serde_json::to_string(&$body).unwrap_or_default()))
            .unwrap()
    };
}

async fn handler(s3: &S3Client, bucket: &str, event: Request) -> Result<Response<Body>, Error> {
    let method = event.method().as_str().to_uppercase();
    let raw_path = event.uri().path().to_string();
    let nm: Option<String> = event.path_parameters().first("name").map(|s| s.to_string());
    let did: Option<String> = event.path_parameters().first("device_id").map(|s| s.to_string());

    info!(method = %method, path = %raw_path, "fw request");

    if method == "POST" && raw_path == "/firmware/status" {
        let body_str = match event.body() {
            Body::Text(t) => t.clone(),
            Body::Binary(b) => String::from_utf8_lossy(b).to_string(),
            Body::Empty => "{}".into(),
        };
        let data: Value = serde_json::from_str(&body_str).unwrap_or(json!({}));
        let dev = data["device_id"].as_str().unwrap_or("").replace('/', "_").replace("..", "_");
        let fw = data["firmware_name"].as_str().unwrap_or("").replace('/', "_").replace("..", "_");
        if dev.is_empty() || fw.is_empty() {
            return Ok(cors_json!(StatusCode::BAD_REQUEST, json!({"error": "need device_id and firmware_name"})));
        }
        let key = format!("status/{dev}/{fw}.json");
        let now = {
            let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
            aws_sdk_s3::primitives::DateTime::from_secs(d.as_secs() as i64)
                .fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime).unwrap_or_default()
        };
        let blob = json!({
            "device_id": dev, "firmware_name": fw,
            "status": data["status"].as_str().unwrap_or("unknown"),
            "version": data["version"].as_str().unwrap_or(""),
            "error": data["error"].as_str().unwrap_or(""),
            "progress": data["progress"].as_u64().unwrap_or(0),
            "updated_at": now,
        });
        s3.put_object().bucket(bucket).key(&key)
            .body(ByteStream::from(serde_json::to_vec(&blob)?))
            .content_type("application/json").send().await?;
        return Ok(cors_json!(StatusCode::OK, json!({"ok": true})));
    }

    // GET /firmware/status/{device_id}
    if method == "GET" && did.is_some() && raw_path.starts_with("/firmware/status/") {
        let device_id = did.unwrap().replace('/', "_").replace("..", "_");
        let prefix = format!("status/{device_id}/");
        let list = s3.list_objects_v2().bucket(bucket).prefix(&prefix).send().await?;
        let mut out: Vec<Value> = Vec::new();
        for obj in list.contents() {
            if let Some(key) = obj.key() {
                let get = s3.get_object().bucket(bucket).key(key).send().await?;
                let bytes = get.body.collect().await?.into_bytes();
                out.push(serde_json::from_slice(&bytes).unwrap_or(json!({})));
            }
        }
        return Ok(cors_json!(StatusCode::OK, json!(out)));
    }

    // GET /firmware
    if method == "GET" && nm.is_none() {
        let mut items: Vec<Value> = Vec::new();
        let mut ct: Option<String> = None;
        loop {
            let mut req = s3.list_objects_v2().bucket(bucket);
            if let Some(ref tok) = ct { req = req.continuation_token(tok); }
            let res = req.send().await?;
            for obj in res.contents() {
                let key = match obj.key() { Some(k) => k, None => continue };
                if key.starts_with("status/") { continue; }
                let head = s3.head_object().bucket(bucket).key(key).send().await?;
                let ids_raw = head.metadata().and_then(|m| m.get("device-ids")).map(|s| s.as_str()).unwrap_or("");
                let ids: Vec<&str> = ids_raw.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                let lm = obj.last_modified()
                    .and_then(|dt| dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime).ok())
                    .unwrap_or_default();
                items.push(json!({
                    "name": key, "lastModified": lm,
                    "size": obj.size().unwrap_or(0), "deviceIds": ids,
                }));
            }
            if !res.is_truncated().unwrap_or(false) { break; }
            ct = res.next_continuation_token().map(|s| s.to_string());
        }
        return Ok(cors_json!(StatusCode::OK, json!(items)));
    }

    // GET /firmware/{name} 
    if method == "GET" && nm.is_some() {
        let name = nm.unwrap().replace('/', "_").replace("..", "_");
        match s3.get_object().bucket(bucket).key(&name).send().await {
            Ok(obj) => {
                let lm = obj.last_modified()
                    .and_then(|dt| dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime).ok())
                    .unwrap_or_default();
                let ids_csv = obj.metadata().and_then(|m| m.get("device-ids")).cloned().unwrap_or_default();
                let raw = obj.body.collect().await?.into_bytes();
                let ids: Vec<&str> = ids_csv.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                return Ok(cors_json!(StatusCode::OK, json!({
                    "name": name, "content": B64.encode(&raw), "size": raw.len(),
                    "deviceIds": ids, "lastModified": lm,
                })));
            }
            Err(e) => {
                let svc = e.into_service_error();
                if svc.is_no_such_key() {
                    return Ok(cors_json!(StatusCode::NOT_FOUND, json!({"error": format!("{name} not in bucket")})));
                }
                return Err(Box::new(svc));
            }
        }
    }

    // PUT /firmware/{name} 
    if method == "PUT" && nm.is_some() {
        let name = nm.unwrap().replace('/', "_").replace("..", "_").trim().to_string();
        if !name.ends_with(".bin") {
            return Ok(cors_json!(StatusCode::BAD_REQUEST, json!({"error": "must be .bin"})));
        }
        let body_str = match event.body() {
            Body::Text(t) => t.clone(),
            Body::Binary(b) => String::from_utf8_lossy(b).to_string(),
            Body::Empty => "{}".into(),
        };
        let data: Value = serde_json::from_str(&body_str).unwrap_or(json!({}));
        let binary = match B64.decode(data["content"].as_str().unwrap_or("")) {
            Ok(b) => b,
            Err(_) => return Ok(cors_json!(StatusCode::BAD_REQUEST, json!({"error": "bad base64"}))),
        };
        if binary.len() > MAX_FIRMWARE_BYTES {
            return Ok(cors_json!(StatusCode::BAD_REQUEST, json!({"error": format!("too big (max {}MB)", MAX_FIRMWARE_BYTES / (1024*1024))})));
        }
        // validate ESP32 
        if let Some(err) = check_esp32_header(&binary) {
            warn!(name = %name, error = %err, "rejected firmware upload");
            return Ok(cors_json!(StatusCode::BAD_REQUEST, json!({"error": err})));
        }
        let ids: Vec<String> = data["deviceIds"].as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        let n = binary.len();
        s3.put_object().bucket(bucket).key(&name)
            .body(ByteStream::from(binary))
            .content_type("application/octet-stream")
            .metadata("device-ids", &ids.join(","))
            .send().await?;
        return Ok(cors_json!(StatusCode::OK, json!({"ok": true, "bytes": n})));
    }

    // DELETE /firmware/{name}
    if method == "DELETE" && nm.is_some() {
        let name = nm.unwrap().replace('/', "_").replace("..", "_");
        s3.delete_object().bucket(bucket).key(&name).send().await?;
        return Ok(cors_json!(StatusCode::OK, json!({"ok": true})));
    }

    Ok(cors_json!(StatusCode::NOT_FOUND, json!({"error": "no route"})))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json().without_time().init();

    let cfg = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let s3 = S3Client::new(&cfg);
    let bucket = env::var("BUCKET_NAME").expect("BUCKET_NAME required");

    run(service_fn(|event: Request| {
        let s3 = &s3;
        let bucket = &bucket;
        async move {
            match handler(s3, bucket, event).await {
                Ok(r) => Ok::<Response<Body>, Error>(r),
                Err(e) => {
                    error!(error = %e, "unhandled");
                    Ok(cors_json!(StatusCode::INTERNAL_SERVER_ERROR, json!({"error": e.to_string()})))
                }
            }
        }
    })).await
}
