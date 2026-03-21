use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_s3::presigning::PresigningConfig;
use lambda_http::{run, service_fn, Body, Request, RequestExt, Response};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::OnceCell;
use tracing::{error, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Config loaded once from environment
// ---------------------------------------------------------------------------
struct Config {
    tailscale_api_key_secret_name: String,
    tailscale_tailnet: String,
    coordinator_host: String,
    coordinator_grpc_port: u16,
    coordinator_rest_port: u16,
    requests_table: String,
    registry_table: String,
    cloudmap_namespace: String,
    mqtt_broker_service: String,
    coordinator_service: String,
    releases_bucket: String,
}

impl Config {
    fn from_env() -> Self {
        Self {
            tailscale_api_key_secret_name: env::var("TAILSCALE_API_KEY_SECRET_NAME")
                .unwrap_or_default(),
            tailscale_tailnet: env::var("TAILSCALE_TAILNET").unwrap_or_default(),
            coordinator_host: env::var("COORDINATOR_HOST").unwrap_or_default(),
            coordinator_grpc_port: env::var("COORDINATOR_GRPC_PORT")
                .unwrap_or_else(|_| "8080".into())
                .parse()
                .unwrap_or(8080),
            coordinator_rest_port: env::var("COORDINATOR_REST_PORT")
                .unwrap_or_else(|_| "8081".into())
                .parse()
                .unwrap_or(8081),
            requests_table: env::var("REQUESTS_TABLE").unwrap_or_default(),
            registry_table: env::var("REGISTRY_TABLE").unwrap_or_default(),
            cloudmap_namespace: env::var("CLOUDMAP_NAMESPACE")
                .unwrap_or_else(|_| "iot.local".into()),
            mqtt_broker_service: env::var("MQTT_BROKER_SERVICE")
                .unwrap_or_else(|_| "mqtt-broker".into()),
            coordinator_service: env::var("COORDINATOR_SERVICE")
                .unwrap_or_else(|_| "nebulastream".into()),
            releases_bucket: env::var("RELEASES_BUCKET")
                .unwrap_or_else(|_| "iot-platform-gateway-releases".into()),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------
struct AppState {
    cfg: Config,
    ddb: aws_sdk_dynamodb::Client,
    s3: aws_sdk_s3::Client,
    sm: aws_sdk_secretsmanager::Client,
    sd: aws_sdk_servicediscovery::Client,
    http: reqwest::Client,
    ts_key: OnceCell<String>,
}

const TS_API: &str = "https://api.tailscale.com";
// bumped from 6h to 30 days — factory gateways reboot after power glitches
// and shouldn't need manual re-approval every time. The admin can still
// revoke via the Cloud Desktop authorization page if needed.
const APPROVAL_TTL: i64 = 30 * 24 * 3600;

const CORS_ORIGIN: &str = "tauri://localhost";

const FP_WEIGHTS: &[(&str, u32)] = &[
    ("mac_address", 30),
    ("cpu_id", 25),
    ("serial_number", 20),
    ("hostname", 15),
    ("os_info", 10),
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn cors_response(status: u16, body: Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("Access-Control-Allow-Origin", CORS_ORIGIN)
        .header("Access-Control-Allow-Headers", "Content-Type,Authorization")
        .header("Access-Control-Allow-Methods", "GET,POST,DELETE,OPTIONS")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap_or_default()))
        .unwrap()
}

fn hash_secret(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn gen_secret() -> String {
    use rand::Rng;
    let bytes: Vec<u8> = (0..32).map(|_| rand::thread_rng().gen()).collect();
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &bytes)
}

fn gw_from_path(path: &str) -> Option<String> {
    let parts: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() < 3 {
        None
    } else {
        Some(parts.last().unwrap().to_string())
    }
}

fn av_s(val: &str) -> AttributeValue {
    AttributeValue::S(val.to_string())
}

fn av_n(val: i64) -> AttributeValue {
    AttributeValue::N(val.to_string())
}

fn av_bool(val: bool) -> AttributeValue {
    AttributeValue::Bool(val)
}

/// Extract a string from a DynamoDB attribute map.
fn get_s(item: &HashMap<String, AttributeValue>, key: &str) -> String {
    item.get(key)
        .and_then(|v| v.as_s().ok())
        .unwrap_or(&String::new())
        .clone()
}

fn get_n(item: &HashMap<String, AttributeValue>, key: &str) -> i64 {
    item.get(key)
        .and_then(|v| v.as_n().ok())
        .and_then(|n| n.parse::<i64>().ok())
        .unwrap_or(0)
}

fn get_bool(item: &HashMap<String, AttributeValue>, key: &str) -> bool {
    item.get(key)
        .and_then(|v| v.as_bool().ok())
        .copied()
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// JWT / Cognito helpers
// ---------------------------------------------------------------------------
fn jwt_groups(event: &Request) -> std::collections::HashSet<String> {
    let mut groups = std::collections::HashSet::new();
    if let Some(ctx) = event.request_context_ref() {
        if let lambda_http::request::RequestContext::ApiGatewayV2(api) = ctx {
            if let Some(auth) = &api.authorizer {
                if let Some(jwt) = &auth.jwt {
                    if let Some(raw) = jwt.claims.get("cognito:groups") {
                        let cleaned = raw.trim_matches(|c: char| c == '[' || c == ']');
                        for g in cleaned.split([',', ' ']) {
                            let g = g.trim();
                            if !g.is_empty() {
                                groups.insert(g.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    groups
}

fn jwt_sub(event: &Request) -> String {
    if let Some(ctx) = event.request_context_ref() {
        if let lambda_http::request::RequestContext::ApiGatewayV2(api) = ctx {
            if let Some(auth) = &api.authorizer {
                if let Some(jwt) = &auth.jwt {
                    if let Some(sub) = jwt.claims.get("sub") {
                        return sub.clone();
                    }
                }
            }
        }
    }
    "unknown".to_string()
}

fn source_ip(event: &Request) -> String {
    if let Some(ctx) = event.request_context_ref() {
        if let lambda_http::request::RequestContext::ApiGatewayV2(api) = ctx {
            return api.http.source_ip.clone().unwrap_or_default();
        }
    }
    String::new()
}

// ---------------------------------------------------------------------------
// Trust scoring
// ---------------------------------------------------------------------------
fn trust_score(
    reported: &HashMap<String, String>,
    expected: &HashMap<String, AttributeValue>,
) -> u32 {
    let mut earned: u32 = 0;
    let mut possible: u32 = 0;

    for (field, weight) in FP_WEIGHTS {
        let exp_key = format!("expected_{field}");
        let exp = get_s(expected, &exp_key).trim().to_lowercase();
        let rep = reported
            .get(*field)
            .map(|s| s.trim().to_lowercase())
            .unwrap_or_default();
        if exp.is_empty() {
            continue;
        }
        possible += weight;
        if exp == rep {
            earned += weight;
        }
    }

    if possible == 0 {
        return 0;
    }
    (earned * 100 + possible / 2) / possible // rounded
}

// ---------------------------------------------------------------------------
// Tailscale API helpers
// ---------------------------------------------------------------------------
impl AppState {
    async fn ts_api_key(&self) -> Result<String, String> {
        self.ts_key
            .get_or_try_init(|| async {
                if self.cfg.tailscale_api_key_secret_name.is_empty() {
                    return Err("TAILSCALE_API_KEY_SECRET_NAME not configured".to_string());
                }
                let resp = self
                    .sm
                    .get_secret_value()
                    .secret_id(&self.cfg.tailscale_api_key_secret_name)
                    .send()
                    .await
                    .map_err(|e| format!("Secrets Manager error: {e}"))?;
                resp.secret_string()
                    .map(|s| s.to_string())
                    .ok_or_else(|| "Secret has no string value".to_string())
            })
            .await
            .cloned()
    }

    async fn ts_call(
        &self,
        path: &str,
        method: &str,
        data: Option<Value>,
    ) -> Result<Value, String> {
        let url = format!("{TS_API}{path}");
        let api_key = self.ts_api_key().await?;

        let builder = match method {
            "POST" => self.http.post(&url),
            "DELETE" => self.http.delete(&url),
            _ => self.http.get(&url),
        };

        let mut builder = builder
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json");

        if let Some(body) = data {
            builder = builder.json(&body);
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| format!("Tailscale HTTP error: {e}"))?;
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(format!("Tailscale API {status}: {body_text}"));
        }

        if body_text.is_empty() {
            return Ok(json!({}));
        }
        serde_json::from_str(&body_text).map_err(|e| format!("JSON parse error: {e}"))
    }

    async fn make_ts_key(&self, gw_id: &str) -> Result<String, String> {
        let payload = json!({
            "capabilities": {
                "devices": {
                    "create": {
                        "reusable": false,
                        "ephemeral": true,
                        "preauthorized": true,
                        "tags": ["tag:gateway"]
                    }
                }
            },
            "expirySeconds": 300,
            "description": format!("Ephemeral key for gateway {gw_id}")
        });

        let path = format!("/api/v2/tailnet/{}/keys", self.cfg.tailscale_tailnet);
        match self.ts_call(&path, "POST", Some(payload.clone())).await {
            Ok(resp) => Ok(resp
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()),
            Err(e) if e.contains("400") && e.to_lowercase().contains("tag") => {
                // Retry without tags
                let mut retry_payload = payload;
                if let Some(cap) = retry_payload.get_mut("capabilities") {
                    if let Some(dev) = cap.get_mut("devices") {
                        if let Some(create) = dev.get_mut("create") {
                            if let Some(obj) = create.as_object_mut() {
                                obj.remove("tags");
                            }
                        }
                    }
                }
                let resp = self.ts_call(&path, "POST", Some(retry_payload)).await?;
                Ok(resp
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string())
            }
            Err(e) => Err(e),
        }
    }

    async fn find_ts_device(&self, gateway_id: &str) -> Result<Option<Value>, String> {
        let path = format!("/api/v2/tailnet/{}/devices", self.cfg.tailscale_tailnet);
        let resp = self.ts_call(&path, "GET", None).await?;
        let devices = resp.get("devices").and_then(|v| v.as_array());
        if let Some(devices) = devices {
            for d in devices {
                let hn = d.get("hostname").and_then(|v| v.as_str()).unwrap_or("");
                if hn == gateway_id || hn.split('.').next().unwrap_or("") == gateway_id {
                    return Ok(Some(d.clone()));
                }
            }
        }
        Ok(None)
    }

    // ---------------------------------------------------------------------------
    // Cloud Map service discovery
    // ---------------------------------------------------------------------------
    async fn resolve_ip(&self, service_name: &str) -> Option<String> {
        if self.cfg.cloudmap_namespace.is_empty() || service_name.is_empty() {
            return None;
        }
        match self
            .sd
            .discover_instances()
            .namespace_name(&self.cfg.cloudmap_namespace)
            .service_name(service_name)
            .max_results(1)
            .health_status(aws_sdk_servicediscovery::types::HealthStatusFilter::Healthy)
            .send()
            .await
        {
            Ok(resp) => {
                let instances = resp.instances();
                if let Some(inst) = instances.first() {
                    if let Some(attrs) = inst.attributes() {
                        if let Some(ip) = attrs.get("AWS_INSTANCE_IPV4") {
                            if !ip.is_empty() {
                                return Some(ip.clone());
                            }
                        }
                    }
                }
                warn!(
                    "No healthy instances for {service_name}.{}",
                    self.cfg.cloudmap_namespace
                );
                None
            }
            Err(e) => {
                error!("Cloud Map resolution failed for {service_name}: {e}");
                None
            }
        }
    }

    // ---------------------------------------------------------------------------
    // DynamoDB helpers
    // ---------------------------------------------------------------------------
    async fn ddb_get(
        &self,
        table: &str,
        key_name: &str,
        key_val: &str,
    ) -> Option<HashMap<String, AttributeValue>> {
        self.ddb
            .get_item()
            .table_name(table)
            .key(key_name, av_s(key_val))
            .send()
            .await
            .ok()
            .and_then(|r| r.item().map(|i| i.clone()))
    }

    async fn ddb_put(
        &self,
        table: &str,
        item: HashMap<String, AttributeValue>,
    ) -> Result<(), String> {
        self.ddb
            .put_item()
            .table_name(table)
            .set_item(Some(item))
            .send()
            .await
            .map_err(|e| format!("DynamoDB put error: {e}"))?;
        Ok(())
    }

    async fn ddb_delete(&self, table: &str, key_name: &str, key_val: &str) -> Result<(), String> {
        self.ddb
            .delete_item()
            .table_name(table)
            .key(key_name, av_s(key_val))
            .send()
            .await
            .map_err(|e| format!("DynamoDB delete error: {e}"))?;
        Ok(())
    }

    async fn ddb_scan(&self, table: &str) -> Vec<HashMap<String, AttributeValue>> {
        self.ddb
            .scan()
            .table_name(table)
            .send()
            .await
            .map(|r| r.items().to_vec())
            .unwrap_or_default()
    }

    // ---------------------------------------------------------------------------
    // Route: POST /vpn/gateways -- register gateway (admin only)
    // ---------------------------------------------------------------------------
    async fn do_register(&self, event: &Request) -> Response<Body> {
        let groups = jwt_groups(event);
        if !groups.contains("administrators") {
            return cors_response(403, json!({"error": "Admin access required"}));
        }

        let body: Value = match parse_body(event) {
            Ok(b) => b,
            Err(r) => return r,
        };

        let gw_id = match body.get("gateway_id").and_then(|v| v.as_str()) {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => return cors_response(400, json!({"error": "Missing required field: gateway_id"})),
        };

        // Check existing
        if self
            .ddb_get(&self.cfg.registry_table, "gateway_id", &gw_id)
            .await
            .is_some()
        {
            return cors_response(
                409,
                json!({"error": format!("Gateway '{}' is already registered", gw_id)}),
            );
        }

        let secret = gen_secret();
        let secret_hash_val = hash_secret(&secret);
        let now = now_epoch();
        let who = jwt_sub(event);

        let mut item: HashMap<String, AttributeValue> = HashMap::new();
        item.insert("gateway_id".into(), av_s(&gw_id));
        item.insert("secret_hash".into(), av_s(&secret_hash_val));
        item.insert("registered_at".into(), av_n(now));
        item.insert("registered_by".into(), av_s(&who));

        for (field, _) in FP_WEIGHTS {
            let key = format!("expected_{field}");
            if let Some(val) = body.get(&key).and_then(|v| v.as_str()) {
                let val = val.trim();
                if !val.is_empty() {
                    item.insert(key, av_s(val));
                }
            }
        }

        if let Some(loc) = body.get("location").and_then(|v| v.as_str()) {
            let loc = loc.trim();
            if !loc.is_empty() {
                item.insert("location".into(), av_s(loc));
            }
        }

        if let Err(e) = self.ddb_put(&self.cfg.registry_table, item).await {
            error!("Failed to register gateway: {e}");
            return cors_response(500, json!({"error": "Internal error"}));
        }

        info!("Registered gateway {gw_id} by user {who}");

        cors_response(201, json!({
            "gateway_id": gw_id,
            "pre_shared_secret": secret,
            "message": "Gateway registered. Save this enrollment token - it will NOT be shown again. Provision it into the gateway's config file as 'pre_shared_secret'. The token is single-use: the gateway will be auto-approved on first boot, and the token is then invalidated."
        }))
    }

    // ---------------------------------------------------------------------------
    // Route: POST /vpn/request -- gateway requests VPN access
    // ---------------------------------------------------------------------------
    async fn handle_request(&self, event: &Request) -> Response<Body> {
        let body: Value = match parse_body(event) {
            Ok(b) => b,
            Err(r) => return r,
        };

        let gateway_id = match body.get("gateway_id").and_then(|v| v.as_str()) {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => return cors_response(400, json!({"error": "Missing required field: gateway_id"})),
        };

        let pre_shared_secret = body
            .get("pre_shared_secret")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let fingerprint: HashMap<String, String> = body
            .get("fingerprint")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let mut trust: u32 = 0;
        let mut reg_ok = false;
        let mut secret_ok = false;

        if !self.cfg.registry_table.is_empty() {
            if let Some(reg_item) = self
                .ddb_get(&self.cfg.registry_table, "gateway_id", &gateway_id)
                .await
            {
                reg_ok = true;
                let stored_hash = get_s(&reg_item, "secret_hash");
                if !pre_shared_secret.is_empty() && !stored_hash.is_empty() {
                    if hash_secret(pre_shared_secret) == stored_hash {
                        secret_ok = true;
                    }
                }
                trust = trust_score(&fingerprint, &reg_item);
            }
        }

        let src_ip = source_ip(event);
        let request_token = Uuid::new_v4().to_string();
        let now = now_epoch();
        let ttl = now + 3600;

        // Check existing request
        if let Some(existing) = self
            .ddb_get(&self.cfg.requests_table, "gateway_id", &gateway_id)
            .await
        {
            let st = get_s(&existing, "status");

            if st == "pending" {
                return cors_response(
                    200,
                    json!({
                        "request_token": get_s(&existing, "request_token"),
                        "status": "pending",
                        "message": "Authorization request already pending"
                    }),
                );
            }

            if st == "approved" {
                let approved_at = get_n(&existing, "approved_at");
                if now - approved_at < APPROVAL_TTL {
                    if let Ok(auth_key) = self.make_ts_key(&gateway_id).await {
                        if !auth_key.is_empty() {
                            let new_tok = Uuid::new_v4().to_string();
                            let _ = self
                                .ddb
                                .update_item()
                                .table_name(&self.cfg.requests_table)
                                .key("gateway_id", av_s(&gateway_id))
                                .update_expression("SET auth_key = :key, request_token = :tok, created_at = :now, #s = :approved, trust_score = :ts, registry_validated = :rv, secret_validated = :sv, approved_at = :at")
                                .expression_attribute_names("#s", "status")
                                .expression_attribute_values(":key", av_s(&auth_key))
                                .expression_attribute_values(":tok", av_s(&new_tok))
                                .expression_attribute_values(":now", av_n(now))
                                .expression_attribute_values(":approved", av_s("approved"))
                                .expression_attribute_values(":ts", av_n(trust as i64))
                                .expression_attribute_values(":rv", av_bool(reg_ok))
                                .expression_attribute_values(":sv", av_bool(secret_ok))
                                .expression_attribute_values(":at", av_n(now))
                                .send()
                                .await;

                            return cors_response(
                                200,
                                json!({
                                    "request_token": new_tok,
                                    "status": "pending",
                                    "message": "Auto-approved (previous approval still valid)"
                                }),
                            );
                        }
                    }
                }
            }
        }

        // Auto approve if:
        // 1. enrollment token matches (first boot), OR
        // 2. gateway is registered in the registry (subsequent boots)
        // We used to require the secret for auto-approve but that meant every
        // reboot after the first went to "pending" and an admin had to click
        // approve manually. For factory floor gateways that reboot after power
        // glitches this was happening multiple times a day.
        if secret_ok || reg_ok {
            if let Ok(auth_key) = self.make_ts_key(&gateway_id).await {
                if !auth_key.is_empty() {
                    let mut item: HashMap<String, AttributeValue> = HashMap::new();
                    item.insert("gateway_id".into(), av_s(&gateway_id));
                    item.insert("request_token".into(), av_s(&request_token));
                    item.insert("status".into(), av_s("approved"));
                    item.insert("created_at".into(), av_n(now));
                    item.insert("ttl".into(), av_n(ttl));
                    item.insert("source_ip".into(), av_s(&src_ip));
                    item.insert("trust_score".into(), av_n(trust as i64));
                    item.insert("registry_validated".into(), av_bool(reg_ok));
                    item.insert("secret_validated".into(), av_bool(secret_ok));
                    item.insert("auth_key".into(), av_s(&auth_key));
                    item.insert("approved_by".into(), av_s(if secret_ok { "auto:enrollment_token" } else { "auto:registered_gateway" }));
                    item.insert("approved_at".into(), av_n(now));

                    if !fingerprint.is_empty() {
                        let fp_val = serde_json::to_string(&fingerprint).unwrap_or_default();
                        item.insert("fingerprint".into(), av_s(&fp_val));
                    }

                    for field in &["hostname", "location"] {
                        if let Some(val) = body.get(*field).and_then(|v| v.as_str()) {
                            if !val.is_empty() {
                                item.insert(field.to_string(), av_s(val));
                            }
                        }
                    }

                    let _ = self.ddb_put(&self.cfg.requests_table, item).await;

                    // Invalidate enrollment token so it can't be reused
                    let _ = self
                        .ddb
                        .update_item()
                        .table_name(&self.cfg.registry_table)
                        .key("gateway_id", av_s(&gateway_id))
                        .update_expression("REMOVE secret_hash SET enrolled_at = :now")
                        .expression_attribute_values(":now", av_n(now))
                        .send()
                        .await;

                    return cors_response(
                        200,
                        json!({
                            "request_token": request_token,
                            "status": "pending",
                            "message": "Auto-approved via enrollment token."
                        }),
                    );
                }
            }
        }

        // Fall through: create pending request
        let mut item: HashMap<String, AttributeValue> = HashMap::new();
        item.insert("gateway_id".into(), av_s(&gateway_id));
        item.insert("request_token".into(), av_s(&request_token));
        item.insert("status".into(), av_s("pending"));
        item.insert("created_at".into(), av_n(now));
        item.insert("ttl".into(), av_n(ttl));
        item.insert("source_ip".into(), av_s(&src_ip));
        item.insert("trust_score".into(), av_n(trust as i64));
        item.insert("registry_validated".into(), av_bool(reg_ok));
        item.insert("secret_validated".into(), av_bool(secret_ok));

        if !fingerprint.is_empty() {
            let fp_val = serde_json::to_string(&fingerprint).unwrap_or_default();
            item.insert("fingerprint".into(), av_s(&fp_val));
        }

        for field in &["hostname", "location"] {
            if let Some(val) = body.get(*field).and_then(|v| v.as_str()) {
                if !val.is_empty() {
                    item.insert(field.to_string(), av_s(val));
                }
            }
        }

        let _ = self.ddb_put(&self.cfg.requests_table, item).await;

        cors_response(
            200,
            json!({
                "request_token": request_token,
                "status": "pending",
                "message": "Authorization request submitted. Waiting for admin approval."
            }),
        )
    }

    // ---------------------------------------------------------------------------
    // Route: GET /vpn/poll/{token}
    // ---------------------------------------------------------------------------
    async fn poll_result(&self, token: &str) -> Response<Body> {
        // Scan by token because gateway doesn't know its dynamo key
        let res = self
            .ddb
            .scan()
            .table_name(&self.cfg.requests_table)
            .filter_expression("request_token = :t")
            .expression_attribute_values(":t", av_s(token))
            .send()
            .await;

        let items = match res {
            Ok(r) => r.items().to_vec(),
            Err(_) => return cors_response(500, json!({"error": "scan failed"})),
        };

        if items.is_empty() {
            return cors_response(404, json!({"error": "expired or not found"}));
        }

        let it = &items[0];
        let st = get_s(it, "status");

        if st == "approved" {
            let cip = self
                .resolve_ip(&self.cfg.coordinator_service)
                .await
                .unwrap_or_else(|| self.cfg.coordinator_host.clone());

            let mut r = json!({
                "status": "approved",
                "auth_key": get_s(it, "auth_key"),
                "tailnet": self.cfg.tailscale_tailnet,
                "coordinator_host": cip,
                "coordinator_grpc_port": self.cfg.coordinator_grpc_port,
                "coordinator_rest_port": self.cfg.coordinator_rest_port
            });

            if let Some(mip) = self.resolve_ip(&self.cfg.mqtt_broker_service).await {
                r["mqtt_broker_host"] = json!(mip);
                r["mqtt_broker_port"] = json!(1883);
            }

            return cors_response(200, r);
        }

        if st == "denied" {
            return cors_response(200, json!({"status": "denied"}));
        }

        cors_response(200, json!({"status": "pending"}))
    }

    // ---------------------------------------------------------------------------
    // Route: POST /vpn/approve/{id}
    // ---------------------------------------------------------------------------
    async fn handle_approve(&self, event: &Request, gw_id: &str) -> Response<Body> {
        let groups = jwt_groups(event);
        if !groups.contains("administrators") && !groups.contains("maintenance") {
            return cors_response(403, json!({"error": "Insufficient permissions"}));
        }

        let item = match self
            .ddb_get(&self.cfg.requests_table, "gateway_id", gw_id)
            .await
        {
            Some(it) => it,
            None => {
                return cors_response(
                    404,
                    json!({"error": format!("No request found for '{gw_id}'")}),
                )
            }
        };

        let status = get_s(&item, "status");
        if status != "pending" {
            return cors_response(
                409,
                json!({"error": format!("Request is already {status}")}),
            );
        }

        let auth_key = match self.make_ts_key(gw_id).await {
            Ok(k) => k,
            Err(_) => {
                return cors_response(
                    502,
                    json!({"error": "Could not create Tailscale auth key"}),
                )
            }
        };

        let who = jwt_sub(event);
        let now = now_epoch();

        let _ = self
            .ddb
            .update_item()
            .table_name(&self.cfg.requests_table)
            .key("gateway_id", av_s(gw_id))
            .update_expression(
                "SET #s = :approved, auth_key = :key, approved_by = :by, approved_at = :at",
            )
            .expression_attribute_names("#s", "status")
            .expression_attribute_values(":approved", av_s("approved"))
            .expression_attribute_values(":key", av_s(&auth_key))
            .expression_attribute_values(":by", av_s(&who))
            .expression_attribute_values(":at", av_n(now))
            .send()
            .await;

        cors_response(
            200,
            json!({"message": format!("Gateway '{}' approved", gw_id), "gateway_id": gw_id}),
        )
    }

    // ---------------------------------------------------------------------------
    // Route: POST /vpn/provision
    // ---------------------------------------------------------------------------
    async fn handle_provision(&self, event: &Request) -> Response<Body> {
        let groups = jwt_groups(event);
        if !groups.contains("administrators") && !groups.contains("maintenance") {
            return cors_response(403, json!({"error": "Insufficient permissions"}));
        }

        let body: Value = match parse_body(event) {
            Ok(b) => b,
            Err(r) => return r,
        };

        let gw_id = match body.get("gateway_id").and_then(|v| v.as_str()) {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => return cors_response(400, json!({"error": "Missing required field: gateway_id"})),
        };

        let auth_key = match self.make_ts_key(&gw_id).await {
            Ok(k) => k,
            Err(_) => {
                return cors_response(
                    502,
                    json!({"error": "Could not create Tailscale auth key"}),
                )
            }
        };

        let cip = self
            .resolve_ip(&self.cfg.coordinator_service)
            .await
            .unwrap_or_else(|| self.cfg.coordinator_host.clone());

        let mut out = json!({
            "auth_key": auth_key,
            "tailnet": self.cfg.tailscale_tailnet,
            "coordinator_host": cip,
            "coordinator_grpc_port": self.cfg.coordinator_grpc_port,
            "coordinator_rest_port": self.cfg.coordinator_rest_port
        });

        if let Some(mip) = self.resolve_ip(&self.cfg.mqtt_broker_service).await {
            out["mqtt_broker_host"] = json!(mip);
            out["mqtt_broker_port"] = json!(1883);
        }

        cors_response(200, out)
    }

    // ---------------------------------------------------------------------------
    // Route: GET /vpn/status/{id}
    // ---------------------------------------------------------------------------
    async fn handle_status(&self, gw_id: &str) -> Response<Body> {
        let dev = match self.find_ts_device(gw_id).await {
            Ok(Some(d)) => d,
            Ok(None) => {
                return cors_response(
                    404,
                    json!({"error": format!("{gw_id} not on tailnet")}),
                )
            }
            Err(_) => return cors_response(502, json!({"error": "tailscale api down"})),
        };

        let addrs = dev.get("addresses").and_then(|v| v.as_array());
        let ts_ip = addrs
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .unwrap_or("");

        cors_response(
            200,
            json!({
                "gateway_id": gw_id,
                "online": dev.get("online").and_then(|v| v.as_bool()).unwrap_or(false),
                "tailscale_ip": ts_ip,
                "hostname": dev.get("hostname").and_then(|v| v.as_str()).unwrap_or(""),
                "last_seen": dev.get("lastSeen").and_then(|v| v.as_str()).unwrap_or("")
            }),
        )
    }

    // ---------------------------------------------------------------------------
    // Route: DELETE /vpn/revoke/{id}
    // ---------------------------------------------------------------------------
    async fn handle_revoke(&self, event: &Request, gw_id: &str) -> Response<Body> {
        if !jwt_groups(event).contains("administrators") {
            return cors_response(403, json!({"error": "admin only"}));
        }

        let dev = match self.find_ts_device(gw_id).await {
            Ok(Some(d)) => d,
            Ok(None) => {
                return cors_response(404, json!({"error": format!("{gw_id} not found")}))
            }
            Err(_) => return cors_response(502, json!({"error": "tailscale api down"})),
        };

        let did = dev.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if let Err(e) = self
            .ts_call(&format!("/api/v2/device/{did}"), "DELETE", None)
            .await
        {
            error!("Tailscale device delete failed: {e}");
            return cors_response(502, json!({"error": "Failed to delete device"}));
        }

        cors_response(200, json!({"ok": true, "device_id": did}))
    }

    // ---------------------------------------------------------------------------
    // Route: GET /vpn/download/{arch}
    // ---------------------------------------------------------------------------
    async fn handle_download(&self, event: &Request, arch: &str) -> Response<Body> {
        let groups = jwt_groups(event);
        if !groups.contains("administrators") && !groups.contains("maintenance") {
            return cors_response(403, json!({"error": "nope"}));
        }

        let key = format!("latest/gateway-{arch}");

        // Check object exists
        let exists = self
            .s3
            .head_object()
            .bucket(&self.cfg.releases_bucket)
            .key(&key)
            .send()
            .await;

        if exists.is_err() {
            return cors_response(404, json!({"error": format!("no binary for {arch}")}));
        }

        let presign_cfg = PresigningConfig::expires_in(std::time::Duration::from_secs(900))
            .expect("valid presign config");

        let url = self
            .s3
            .get_object()
            .bucket(&self.cfg.releases_bucket)
            .key(&key)
            .presigned(presign_cfg)
            .await;

        match url {
            Ok(presigned) => cors_response(
                200,
                json!({
                    "download_url": presigned.uri(),
                    "architecture": arch
                }),
            ),
            Err(e) => {
                error!("Presign failed: {e}");
                cors_response(500, json!({"error": "Failed to generate download URL"}))
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Route: GET /vpn/gateways -- list all registered gateways
    // ---------------------------------------------------------------------------
    async fn list_gateways(&self, event: &Request) -> Response<Body> {
        let groups = jwt_groups(event);
        if !groups.contains("administrators") && !groups.contains("maintenance") {
            return cors_response(403, json!({"error": "nope"}));
        }

        let items = self.ddb_scan(&self.cfg.registry_table).await;
        let gateways: Vec<Value> = items.iter().map(|i| strip_secret(i)).collect();
        cors_response(200, json!({"gateways": gateways}))
    }

    // ---------------------------------------------------------------------------
    // Route: GET /vpn/gateways/{id} -- get single gateway
    // ---------------------------------------------------------------------------
    async fn get_gateway(&self, event: &Request, gw_id: &str) -> Response<Body> {
        let groups = jwt_groups(event);
        if !groups.contains("administrators") && !groups.contains("maintenance") {
            return cors_response(403, json!({"error": "nope"}));
        }

        match self
            .ddb_get(&self.cfg.registry_table, "gateway_id", gw_id)
            .await
        {
            Some(it) => cors_response(200, json!({"gateway": strip_secret(&it)})),
            None => cors_response(404, json!({"error": format!("{gw_id}?")})),
        }
    }

    // ---------------------------------------------------------------------------
    // Route: DELETE /vpn/gateways/{id} -- delete gateway (admin only)
    // ---------------------------------------------------------------------------
    async fn delete_gateway(&self, event: &Request, gw_id: &str) -> Response<Body> {
        if !jwt_groups(event).contains("administrators") {
            return cors_response(403, json!({"error": "admin only"}));
        }

        if self
            .ddb_get(&self.cfg.registry_table, "gateway_id", gw_id)
            .await
            .is_none()
        {
            return cors_response(404, json!({"error": format!("{gw_id} not registered")}));
        }

        let _ = self
            .ddb_delete(&self.cfg.registry_table, "gateway_id", gw_id)
            .await;

        cors_response(200, json!({"ok": true}))
    }

    // ---------------------------------------------------------------------------
    // Route: GET /vpn/requests -- list pending requests
    // ---------------------------------------------------------------------------
    async fn list_requests(&self, event: &Request) -> Response<Body> {
        let groups = jwt_groups(event);
        if !groups.contains("administrators") && !groups.contains("maintenance") {
            return cors_response(403, json!({"error": "nope"}));
        }

        let res = self
            .ddb
            .scan()
            .table_name(&self.cfg.requests_table)
            .filter_expression("#s = :p")
            .expression_attribute_names("#s", "status")
            .expression_attribute_values(":p", av_s("pending"))
            .send()
            .await;

        let items = match res {
            Ok(r) => r.items().to_vec(),
            Err(_) => return cors_response(500, json!({"error": "scan failed"})),
        };

        let mut out: Vec<Value> = Vec::new();
        for it in &items {
            let mut r = json!({
                "gateway_id": get_s(it, "gateway_id"),
                "created_at": get_n(it, "created_at"),
                "status": "pending",
                "source_ip": get_s(it, "source_ip"),
                "hostname": get_s(it, "hostname"),
                "trust_score": get_n(it, "trust_score"),
                "registry_validated": get_bool(it, "registry_validated"),
                "secret_validated": get_bool(it, "secret_validated")
            });
            let fp = get_s(it, "fingerprint");
            if !fp.is_empty() {
                if let Ok(fp_val) = serde_json::from_str::<Value>(&fp) {
                    r["fingerprint"] = fp_val;
                }
            }
            out.push(r);
        }

        cors_response(200, json!({"requests": out}))
    }
}

// ---------------------------------------------------------------------------
// Utility: strip secret_hash from registry items for API responses
// ---------------------------------------------------------------------------
fn strip_secret(item: &HashMap<String, AttributeValue>) -> Value {
    let mut map = serde_json::Map::new();
    for (k, v) in item {
        if k == "secret_hash" {
            continue;
        }
        if let Ok(s) = v.as_s() {
            map.insert(k.clone(), json!(s));
        } else if let Ok(n) = v.as_n() {
            if let Ok(i) = n.parse::<i64>() {
                map.insert(k.clone(), json!(i));
            } else {
                map.insert(k.clone(), json!(n));
            }
        } else if let Ok(b) = v.as_bool() {
            map.insert(k.clone(), json!(b));
        }
    }
    Value::Object(map)
}

fn parse_body(event: &Request) -> Result<Value, Response<Body>> {
    let raw = match event.body() {
        Body::Text(s) => s.clone(),
        Body::Binary(b) => String::from_utf8_lossy(b).to_string(),
        Body::Empty => "{}".to_string(),
    };
    serde_json::from_str(&raw)
        .map_err(|_| cors_response(400, json!({"error": "Invalid JSON body"})))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------
async fn handler(
    state: Arc<AppState>,
    event: Request,
) -> Result<Response<Body>, lambda_http::Error> {
    // Extract method and path from the HTTP request directly
    let method = event.method().to_string().to_uppercase();
    let path = event.uri().path().to_string();

    // CORS preflight
    if method == "OPTIONS" {
        return Ok(cors_response(200, json!({})));
    }

    // --- /vpn/gateways routes ---
    if path.starts_with("/vpn/gateways") {
        let gid = gw_from_path(&path);

        if method == "POST" && gid.is_none() {
            return Ok(state.do_register(&event).await);
        }

        if method == "GET" && gid.is_none() {
            return Ok(state.list_gateways(&event).await);
        }

        if method == "GET" {
            if let Some(id) = &gid {
                return Ok(state.get_gateway(&event, id).await);
            }
        }

        if method == "DELETE" {
            if let Some(id) = &gid {
                return Ok(state.delete_gateway(&event, id).await);
            }
        }
    }

    // --- /vpn/request ---
    if method == "POST" && path.starts_with("/vpn/request") {
        return Ok(state.handle_request(&event).await);
    }

    // --- /vpn/poll/{token} ---
    if method == "GET" && path.starts_with("/vpn/poll/") {
        let bits: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|s: &&str| !s.is_empty())
            .collect();
        if bits.len() < 3 {
            return Ok(cors_response(400, json!({"error": "missing token"})));
        }
        return Ok(state.poll_result(bits.last().unwrap()).await);
    }

    // --- /vpn/requests ---
    if method == "GET" && path.starts_with("/vpn/requests") {
        return Ok(state.list_requests(&event).await);
    }

    // --- /vpn/approve/{id} ---
    if method == "POST" && path.starts_with("/vpn/approve/") {
        let gw_id = match gw_from_path(&path) {
            Some(id) => id,
            None => {
                return Ok(cors_response(
                    400,
                    json!({"error": "Missing gateway_id in path"}),
                ))
            }
        };
        return Ok(state.handle_approve(&event, &gw_id).await);
    }

    // --- /vpn/provision ---
    if method == "POST" && path.starts_with("/vpn/provision") {
        return Ok(state.handle_provision(&event).await);
    }

    // --- /vpn/status/{id} ---
    if method == "GET" && path.starts_with("/vpn/status/") {
        let gw_id = match gw_from_path(&path) {
            Some(id) => id,
            None => return Ok(cors_response(400, json!({"error": "missing gw id"}))),
        };
        return Ok(state.handle_status(&gw_id).await);
    }

    // --- /vpn/revoke/{id} ---
    if method == "DELETE" && path.starts_with("/vpn/revoke/") {
        let gw_id = match gw_from_path(&path) {
            Some(id) => id,
            None => return Ok(cors_response(400, json!({"error": "missing gw id"}))),
        };
        return Ok(state.handle_revoke(&event, &gw_id).await);
    }

    // --- /vpn/download/{arch} ---
    if method == "GET" && path.starts_with("/vpn/download/") {
        let bits: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|s: &&str| !s.is_empty())
            .collect();
        let arch = if bits.len() >= 3 {
            bits.last().unwrap()
        } else {
            "linux-arm64"
        };
        return Ok(state.handle_download(&event, arch).await);
    }

    Ok(cors_response(404, json!({"error": "no route"})))
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
#[tokio::main]
async fn main() -> Result<(), lambda_http::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .without_time()
        .init();

    let aws_cfg = aws_config::load_from_env().await;

    let state = Arc::new(AppState {
        cfg: Config::from_env(),
        ddb: aws_sdk_dynamodb::Client::new(&aws_cfg),
        s3: aws_sdk_s3::Client::new(&aws_cfg),
        sm: aws_sdk_secretsmanager::Client::new(&aws_cfg),
        sd: aws_sdk_servicediscovery::Client::new(&aws_cfg),
        http: reqwest::Client::new(),
        ts_key: OnceCell::new(),
    });

    run(service_fn(move |event: Request| {
        let state = Arc::clone(&state);
        async move { handler(state, event).await }
    }))
    .await
}
