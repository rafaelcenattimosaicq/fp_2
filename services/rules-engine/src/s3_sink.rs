use std::time::Duration;

use aws_sdk_s3::Client as S3Client;
use serde_json::Value;
use tokio::sync::mpsc;

const BUFFER_HARD_CAP: usize = 10_000;
const FLUSH_RETRY_DELAY_MS: u64 = 2_000; // cellular gateways are slow

#[derive(Debug, Clone)]
pub struct S3SinkConfig {
    pub bucket: String,
    pub key_prefix: String,
    pub flush_interval_secs: u64,
    pub flush_max_records: usize,
}

impl S3SinkConfig {
    pub fn from_env() -> Option<Self> {
        let b = std::env::var("S3_BUCKET_NAME").ok().filter(|s| !s.is_empty())?;

        // s3 flushes every 5min by default
        let kp = std::env::var("S3_KEY_PREFIX")
            .unwrap_or_else(|_| "telemetry".to_string());

        let interval = std::env::var("S3_FLUSH_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);
        let max_r = std::env::var("S3_FLUSH_MAX_RECORDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000);

        Some(Self { bucket: b, key_prefix: kp, flush_interval_secs: interval, flush_max_records: max_r })
    }
}


fn normalize_payload(raw: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(raw).ok()?;
    let thing = v.as_object()?;

    let mut res = serde_json::Map::new();
    for (k, val) in thing {
        res.insert(to_snake(k), val.clone());
    }
    let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    res.insert("ingest_ts".to_string(), Value::String(ts));
    serde_json::to_string(&Value::Object(res)).ok()
}

// works for now, might need to handle more edge cases
fn to_snake(s: &str) -> String {
    let mut buf = String::with_capacity(s.len() + 4);
    let mut was_up = false;
    let mut was_sep = true;

    for (idx, c) in s.chars().enumerate() {
        if c == '_' || c == '-' {
            buf.push('_');
            was_sep = true;
            was_up = false;
            continue;
        }
        if c.is_uppercase() {
            if !was_sep && idx > 0 {
                let nxt = s.chars().nth(idx + 1).map_or(false, |x| x.is_lowercase());
                if !was_up || nxt {
                    buf.push('_');
                }
            }
            buf.push(c.to_lowercase().next().unwrap_or(c));
            was_up = true;
        } else {
            buf.push(c);
            was_up = false;
        }
        was_sep = false;
    }
    buf
}

fn gen_key(prefix: &str) -> String {
    let n = chrono::Utc::now();
    let id = uuid::Uuid::new_v4();
    // hive-style partitioning for athena queries
    format!(
        "{prefix}/year={}/month={:02}/day={:02}/hour={:02}/{id}.jsonl",
        n.format("%Y"), n.format("%m"), n.format("%d"), n.format("%H"),
    )
}

pub fn spawn_s3_sink(config: S3SinkConfig, s3_client: S3Client) -> mpsc::Sender<Vec<u8>> {
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(2048);

    tokio::spawn(async move {
        let mut buf: Vec<String> = Vec::with_capacity(config.flush_max_records);
        let dur = Duration::from_secs(config.flush_interval_secs);
        let mut t = tokio::time::interval(dur);
        t.tick().await; // first tick is instant

        loop {
            tokio::select! {
                item = rx.recv() => {
                    match item {
                        Some(data) => {
                            if let Some(s) = normalize_payload(&data) {
                                buf.push(s);
                            }
                            if buf.len() >= config.flush_max_records {
                                do_flush(&s3_client, &config, &mut buf).await;
                            }
                        }
                        None => {
                            // channel closed, flush remaining
                            if !buf.is_empty() {
                                do_flush(&s3_client, &config, &mut buf).await;
                            }
                            break;
                        }
                    }
                }
                _ = t.tick() => {
                    if !buf.is_empty() {
                        do_flush(&s3_client, &config, &mut buf).await;
                    }
                }
            }
        }
    });

    tx
}


// TODO: maybe use multipart upload for large batches?
async fn do_flush(client: &S3Client, cfg2: &S3SinkConfig, buf: &mut Vec<String>) {
    let k = gen_key(&cfg2.key_prefix);
    let tmp = buf.join("\n");
    let bytes = tmp.into_bytes();

    let res = client
        .put_object()
        .bucket(&cfg2.bucket)
        .key(&k)
        .body(bytes.clone().into())
        .content_type("application/x-ndjson")
        .send()
        .await;

    // retry once, 4G modems drop first attempt sometimes
    let res = match res {
        Ok(r) => Ok(r),
        Err(_e) => {
            tokio::time::sleep(Duration::from_millis(FLUSH_RETRY_DELAY_MS)).await;
            client
                .put_object()
                .bucket(&cfg2.bucket)
                .key(&k)
                .body(bytes.into())
                .content_type("application/x-ndjson")
                .send()
                .await
        }
    };

    match res {
        Ok(_) => { buf.clear(); }
        Err(_e) => {
            // dont let the buffer grow forever
            if buf.len() > BUFFER_HARD_CAP {
                let n = buf.len() - BUFFER_HARD_CAP;
                buf.drain(..n);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_pascal() {
        assert_eq!(to_snake("CompressorSpeed"), "compressor_speed");
    }

    #[test]
    fn snake_case_upper() {
        assert_eq!(to_snake("TEMPERATURE"), "temperature");
    }

    #[test]
    fn snake_case_camel() {
        assert_eq!(to_snake("stateOfCharge"), "state_of_charge");
    }

    #[test]
    fn snake_case_already_snake() {
        assert_eq!(to_snake("device_id"), "device_id");
    }

    #[test]
    fn normalize_adds_ingest_ts() {
        let raw = br#"{"TEMPERATURE": 42.5, "DeviceId": "dev-1"}"#;
        let result = normalize_payload(raw).expect("should normalize");
        let parsed: Value = serde_json::from_str(&result).expect("valid json");
        let obj = parsed.as_object().expect("object");

        assert!(obj.contains_key("temperature"));
        assert!(obj.contains_key("device_id"));
        assert!(obj.contains_key("ingest_ts"));
    }

    #[test]
    fn normalize_rejects_invalid_json() {
        assert!(normalize_payload(b"not json").is_none());
    }

    #[test]
    fn normalize_rejects_non_object() {
        assert!(normalize_payload(b"[1, 2, 3]").is_none());
    }

    #[test]
    fn s3_key_has_hive_partitions() {
        let key = gen_key("telemetry");
        assert!(key.starts_with("telemetry/year="));
        assert!(key.contains("/month="));
        assert!(key.contains("/day="));
        assert!(key.contains("/hour="));
        assert!(key.ends_with(".jsonl"));
    }

    #[test]
    fn snake_case_acronym() {
        assert_eq!(to_snake("XMLParser"), "xml_parser");
    }

    #[test]
    fn snake_case_all_upper_short() {
        assert_eq!(to_snake("HTTP"), "http");
    }

    #[test]
    fn snake_case_single_char_upper() {
        assert_eq!(to_snake("A"), "a");
    }

    #[test]
    fn snake_case_empty_string() {
        assert_eq!(to_snake(""), "");
    }

    #[test]
    fn snake_case_converts_hyphens() {
        assert_eq!(to_snake("device-id"), "device_id");
    }

    #[test]
    fn snake_case_mixed_separators() {
        assert_eq!(to_snake("Foo_BarBaz"), "foo_bar_baz");
    }

    #[test]
    fn snake_case_already_lowercase() {
        assert_eq!(to_snake("temperature"), "temperature");
    }

    #[test]
    fn snake_case_with_numbers() {
        assert_eq!(to_snake("sensor1Value"), "sensor1_value");
    }

    #[test]
    fn normalize_empty_object_adds_only_ingest_ts() {
        let raw = b"{}";
        let result = normalize_payload(raw).expect("should normalize");
        let parsed: Value = serde_json::from_str(&result).expect("valid json");
        let obj = parsed.as_object().expect("object");
        assert_eq!(
            obj.len(),
            1,
            "empty input object should produce only ingest_ts"
        );
        assert!(obj.contains_key("ingest_ts"));
    }

    #[test]
    fn normalize_preserves_numeric_values() {
        let raw = br#"{"Temperature": 42.567}"#;
        let result = normalize_payload(raw).expect("should normalize");
        let parsed: Value = serde_json::from_str(&result).expect("valid json");
        let temp = parsed.get("temperature").expect("temperature key");
        assert_eq!(temp.as_f64().expect("f64"), 42.567);
    }

    #[test]
    fn normalize_handles_nested_objects() {
        let raw = br#"{"SensorData": {"InnerField": 10}}"#;
        let result = normalize_payload(raw).expect("should normalize");
        let parsed: Value = serde_json::from_str(&result).expect("valid json");
        assert!(parsed.get("sensor_data").is_some());
        let inner = parsed.get("sensor_data").unwrap().get("InnerField");
        assert!(inner.is_some(), "nested keys should be preserved");
    }

    #[test]
    fn normalize_rejects_json_string() {
        assert!(normalize_payload(br#""just a string""#).is_none());
    }

    #[test]
    fn normalize_rejects_json_number() {
        assert!(normalize_payload(b"42").is_none());
    }

    #[test]
    fn normalize_ingest_ts_is_iso8601() {
        let raw = br#"{"temp": 1}"#;
        let result = normalize_payload(raw).expect("should normalize");
        let parsed: Value = serde_json::from_str(&result).expect("valid json");
        let ts = parsed
            .get("ingest_ts")
            .expect("ingest_ts")
            .as_str()
            .expect("string");
        assert!(ts.contains('T'), "ISO 8601 timestamps contain 'T'");
        assert!(ts.ends_with('Z'), "UTC timestamps end with 'Z'");
    }

    #[test]
    fn s3_key_uses_custom_prefix() {
        let key = gen_key("my-custom-prefix");
        assert!(
            key.starts_with("my-custom-prefix/year="),
            "key should start with the provided prefix"
        );
    }

    #[test]
    fn s3_key_is_unique_across_calls() {
        let key1 = gen_key("telemetry");
        let key2 = gen_key("telemetry");
        assert_ne!(key1, key2, "each key should have a unique UUID");
    }

    #[test]
    fn s3_config_none_without_bucket() {
        std::env::remove_var("S3_BUCKET_NAME");
        std::env::remove_var("S3_KEY_PREFIX");
        std::env::remove_var("S3_FLUSH_INTERVAL_SECS");
        std::env::remove_var("S3_FLUSH_MAX_RECORDS");

        let config = S3SinkConfig::from_env();
        assert!(config.is_none(), "should return None without S3_BUCKET_NAME");
    }

    #[test]
    fn s3_config_none_with_empty_bucket() {
        std::env::set_var("S3_BUCKET_NAME", "");
        let config = S3SinkConfig::from_env();
        assert!(config.is_none(), "should return None with empty S3_BUCKET_NAME");
        std::env::remove_var("S3_BUCKET_NAME");
    }

    #[test]
    fn s3_config_uses_defaults() {
        std::env::set_var("S3_BUCKET_NAME", "test-bucket");
        std::env::remove_var("S3_KEY_PREFIX");
        std::env::remove_var("S3_FLUSH_INTERVAL_SECS");
        std::env::remove_var("S3_FLUSH_MAX_RECORDS");

        let config = S3SinkConfig::from_env().expect("should return Some with bucket set");
        assert_eq!(config.bucket, "test-bucket");
        assert_eq!(config.key_prefix, "telemetry", "default prefix");
        assert_eq!(config.flush_interval_secs, 300, "default interval is 5 min");
        assert_eq!(config.flush_max_records, 1000, "default max records");

        std::env::remove_var("S3_BUCKET_NAME");
    }

    #[test]
    fn s3_config_reads_custom_env_values() {
        std::env::set_var("S3_BUCKET_NAME", "my-bucket");
        std::env::set_var("S3_KEY_PREFIX", "raw-data");
        std::env::set_var("S3_FLUSH_INTERVAL_SECS", "60");
        std::env::set_var("S3_FLUSH_MAX_RECORDS", "500");

        let config = S3SinkConfig::from_env().expect("should parse");
        assert_eq!(config.bucket, "my-bucket");
        assert_eq!(config.key_prefix, "raw-data");
        assert_eq!(config.flush_interval_secs, 60);
        assert_eq!(config.flush_max_records, 500);

        std::env::remove_var("S3_BUCKET_NAME");
        std::env::remove_var("S3_KEY_PREFIX");
        std::env::remove_var("S3_FLUSH_INTERVAL_SECS");
        std::env::remove_var("S3_FLUSH_MAX_RECORDS");
    }

    #[test]
    fn s3_config_ignores_non_numeric_values() {
        std::env::set_var("S3_BUCKET_NAME", "bucket");
        std::env::set_var("S3_FLUSH_INTERVAL_SECS", "not-a-number");
        std::env::set_var("S3_FLUSH_MAX_RECORDS", "abc");

        let config = S3SinkConfig::from_env().expect("should parse");
        assert_eq!(config.flush_interval_secs, 300, "should fall back to default");
        assert_eq!(config.flush_max_records, 1000, "should fall back to default");

        std::env::remove_var("S3_BUCKET_NAME");
        std::env::remove_var("S3_FLUSH_INTERVAL_SECS");
        std::env::remove_var("S3_FLUSH_MAX_RECORDS");
    }
}
