
#[allow(dead_code)]
const MODBUS_ADDR_MIN: u8 = 0x01;
#[allow(dead_code)]
const MODBUS_ADDR_MAX: u8 = 0xF7;

// Protocols the NES gateway firmware currently supports.
#[allow(dead_code)]
const VALID_PROTOCOLS: &[&str] = &["modbus-rtu", "ble", "mqtt"];


const _CONSISTENT_READ_NOTE: () = ();

use aws_sdk_dynamodb::Client as DynamoClient;
use aws_sdk_dynamodb::types::AttributeValue;
use lambda_http::{run, service_fn, Body, Error, Request, RequestExt, Response};
use serde_json::{json, Map, Value};
use std::env;
use tracing::error;


fn json_response(status: u16, body: Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Methods", "GET,PUT,DELETE,OPTIONS")
        .body(Body::Text(body.to_string()))
        .expect("failed to build response")
}

fn device_id(event: &Request) -> Option<String> {
    event
        .path_parameters()
        .first("device_id")
        .map(|s: &str| s.to_string())
}

async fn handler(event: Request, client: &DynamoClient, table: &str) -> Result<Response<Body>, Error> {
    let method = event.method().as_str().to_uppercase();
    let path = event.raw_http_path();
    let dev = device_id(&event);

    if method == "OPTIONS" {
        return Ok(json_response(200, json!({})));
    }

    let result: Result<Response<Body>, Error> = async {
        // GET /devices/{id}/descriptor -- return raw YAML (Modbus register map)
        if method == "GET" && dev.is_some() && path.ends_with("/descriptor") {
            let dev = dev.as_deref().unwrap();
            let resp = client
                .get_item()
                .table_name(table)
                .key("device_id", AttributeValue::S(dev.to_string()))
                .send()
                .await?;

            let item = resp.item();
            let descriptor = item.and_then(|it| it.get("descriptor")).and_then(|v| {
                if let AttributeValue::S(s) = v { Some(s.as_str()) } else { None }
            });

            return match descriptor {
                Some(yaml) => Ok(Response::builder()
                    .status(200)
                    .header("Content-Type", "application/x-yaml")
                    .body(Body::Text(yaml.to_string()))
                    .expect("failed to build response")),
                None => Ok(Response::builder()
                    .status(404)
                    .header("Content-Type", "text/plain")
                    .body(Body::Text(format!("no descriptor for {dev}")))
                    .expect("failed to build response")),
            };
        }

        // GET /devices/{id} 
        if method == "GET" && dev.is_some() {
            let dev = dev.as_deref().unwrap();
            let resp = client
                .get_item()
                .table_name(table)
                .key("device_id", AttributeValue::S(dev.to_string()))
                .consistent_read(true)
                .send()
                .await?;

            return match resp.item() {
                Some(item) => Ok(json_response(200, item_to_json(item))),
                None => Ok(json_response(404, json!({"error": format!("{dev} not found")}))),
            };
        }

        // GET /devices
        if method == "GET" {
            let resp = client.scan().table_name(table).send().await?;
            let items: Vec<Value> = resp
                .items()
                .iter()
                .map(|it| item_to_json(it))
                .collect();
            return Ok(json_response(200, Value::Array(items)));
        }

        // PUT /devices/{i
        if method == "PUT" {
            if let Some(dev) = dev.as_deref() {
                let raw = event.body();
                let body_str = match raw {
                    Body::Text(t) => t.clone(),
                    Body::Binary(b) => String::from_utf8_lossy(b).to_string(),
                    Body::Empty => {
                        return Ok(json_response(400, json!({"error": "empty body"})));
                    }
                };

                let body: Value = match serde_json::from_str(&body_str) {
                    Ok(v) => v,
                    Err(_) => return Ok(json_response(400, json!({"error": "bad json"}))),
                };

                let protocol = body
                    .get("protocol")
                    .and_then(|v| v.as_str())
                    .unwrap_or("modbus")
                    .to_string();

                let mut item = std::collections::HashMap::new();
                item.insert("device_id".to_string(), AttributeValue::S(dev.to_string()));
                item.insert("protocol".to_string(), AttributeValue::S(protocol));

                if let Some(icon) = body.get("icon").and_then(|v| v.as_str()) {
                    item.insert("icon".to_string(), AttributeValue::S(icon.to_string()));
                }

                if let Some(desc) = body.get("descriptor").and_then(|v| v.as_str()) {
                    item.insert("descriptor".to_string(), AttributeValue::S(desc.to_string()));
                }

                client.put_item().table_name(table).set_item(Some(item)).send().await?;
                return Ok(json_response(200, json!({"ok": true})));
            }
        }

        // DELETE /devices/{id}
        if method == "DELETE" {
            if let Some(dev) = dev.as_deref() {
                client
                    .delete_item()
                    .table_name(table)
                    .key("device_id", AttributeValue::S(dev.to_string()))
                    .send()
                    .await?;
                return Ok(json_response(200, json!({"ok": true})));
            }
        }

        // No matching route
        Ok(json_response(404, json!({"error": "no matching route"})))
    }
    .await;

    match result {
        Ok(resp) => Ok(resp),
        Err(e) => {
            error!(error = %e, "unhandled");
            Ok(json_response(500, json!({"error": e.to_string()})))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .without_time() // Lambda CloudWatch adds its own timestamps
        .init();

    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = DynamoClient::new(&config);
    let table = env::var("TABLE_NAME").expect("TABLE_NAME env var is required");

    run(service_fn(|req| handler(req, &client, &table))).await
}


fn attr_to_json(av: &AttributeValue) -> Value {
    match av {
        AttributeValue::S(s) => Value::String(s.clone()),
        AttributeValue::N(n) => {
            if let Ok(i) = n.parse::<i64>() {
                Value::Number(i.into())
            } else if let Ok(f) = n.parse::<f64>() {
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .unwrap_or(Value::String(n.clone()))
            } else {
                Value::String(n.clone())
            }
        }
        AttributeValue::Bool(b) => Value::Bool(*b),
        AttributeValue::Null(_) => Value::Null,
        AttributeValue::L(list) => Value::Array(list.iter().map(attr_to_json).collect()),
        AttributeValue::M(map) => {
            let obj: Map<String, Value> = map.iter().map(|(k, v)| (k.clone(), attr_to_json(v))).collect();
            Value::Object(obj)
        }
        AttributeValue::Ss(ss) => {
            Value::Array(ss.iter().map(|s| Value::String(s.clone())).collect())
        }
        _ => Value::Null,
    }
}

fn item_to_json(item: &std::collections::HashMap<String, AttributeValue>) -> Value {
    let obj: Map<String, Value> = item.iter().map(|(k, v)| (k.clone(), attr_to_json(v))).collect();
    Value::Object(obj)
}
