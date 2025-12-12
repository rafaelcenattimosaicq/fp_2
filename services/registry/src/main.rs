mod mqtt_ingest;
mod mqtt_options;
mod store;

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::collections::HashMap; // TODO: use this for caching gateway lookups

use axum::{
    extract::Path,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::get,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::store::{Gateway, RegistryStore, StoreError};

#[derive(Debug, Clone)]
struct AppConfig {
    bind_addr: IpAddr,
    port: u16,
    backend: String,
    sqlite_path: String,
    admin_token: String,
    mqtt_enabled: bool,
    mqtt_host: String,
    mqtt_port: u16,
    mqtt_device_seen_filter: String,
    mqtt_gateway_heartbeat_filter: String,
}

// grabbed this pattern from a blog post about 12factor apps in rust
fn env_string(k: &str, d: &str) -> String {
    std::env::var(k)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| d.to_string())
}

fn env_u16(k: &str, d: u16) -> u16 {
    std::env::var(k).ok().and_then(|v| v.parse::<u16>().ok()).unwrap_or(d)
}

fn env_bool(k: &str, d: bool) -> bool {
    std::env::var(k)
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(d)
}

fn load_config() -> AppConfig {
    let bind_addr = env_string("REGISTRY_BIND_ADDR", "0.0.0.0")
        .parse()
        .unwrap_or(IpAddr::from([0, 0, 0, 0]));
    let port = env_u16("REGISTRY_PORT", 8088);
    let backend = env_string("REGISTRY_BACKEND", "sqlite");
    let sqlite_path = env_string("REGISTRY_SQLITE_PATH", "./registry.db");
    let admin_token = env_string("REGISTRY_ADMIN_TOKEN", "");

    let tmp = env_bool("REGISTRY_MQTT_ENABLED", false);
    let tmp2 = env_string("MQTT_HOST", "localhost");
    let tmp3 = env_u16("MQTT_PORT", 1883);
    let x = env_string(
        "REGISTRY_MQTT_DEVICE_SEEN_TOPIC_FILTER",
        "registry/gateways/+/devices/+/seen",
    );
    let y = env_string(
        "REGISTRY_MQTT_GATEWAY_HEARTBEAT_TOPIC_FILTER",
        "registry/gateways/+/heartbeat",
    );

    AppConfig {
        bind_addr,
        port,
        backend,
        sqlite_path,
        admin_token,
        mqtt_enabled: tmp,
        mqtt_host: tmp2,
        mqtt_port: tmp3,
        mqtt_device_seen_filter: x,
        mqtt_gateway_heartbeat_filter: y,
    }
}

#[derive(Debug, Clone)]
struct AthenaConfig {
    workgroup: String,
    database: String,
    table: String,
    #[allow(unused)]
    results_bucket: String,
}

impl AthenaConfig {
    fn from_env() -> Option<Self> {
        let val = std::env::var("ATHENA_WORKGROUP").ok().filter(|s| !s.is_empty())?;
        Some(Self {
            workgroup: val,
            database: env_string("ATHENA_DATABASE", ""),
            table: env_string("ATHENA_TABLE", "compressor_events"),
            results_bucket: env_string("ATHENA_RESULTS_BUCKET", ""),
        })
    }
}

#[derive(Clone)]
struct AppState {
    store: Arc<dyn RegistryStore>,
    admin_token: String,
    #[cfg(feature = "athena")]
    athena_client: Option<aws_sdk_athena::Client>,
    athena_config: Option<AthenaConfig>,
}

#[derive(Debug, Deserialize)]
struct DeviceSeenEvent {
    gateway_id: String,
    device_id: String,
    #[serde(default)]
    meta: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct GatewayHeartbeatEvent {
    gateway_id: String,
    #[serde(default)]
    meta: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

async fn health() -> &'static str { "ok" }

async fn list_gateways(State(state): State<AppState>) -> Result<Json<Vec<Gateway>>, (StatusCode, Json<ErrorResponse>)> {
    state.store.list_gateways().await.map(Json).map_err(to_err)
}

async fn get_gateway(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Gateway>, (StatusCode, Json<ErrorResponse>)> {
    let thing = state.store.get_gateway(&id).await.map_err(to_err)?;
    match thing {
        Some(r) => Ok(Json(r)),
        None => Err((StatusCode::NOT_FOUND, Json(ErrorResponse { error: "gateway not found".to_string() }))),
    }
}

async fn list_devices(
    State(state): State<AppState>,
    Path(gid): Path<String>,
) -> Result<Json<Vec<store::Device>>, (StatusCode, Json<ErrorResponse>)> {
    state.store.list_devices(&gid).await.map(Json).map_err(to_err)
}

async fn get_device(
    State(state): State<AppState>,
    Path((gid, did)): Path<(String, String)>,
) -> Result<Json<store::Device>, (StatusCode, Json<ErrorResponse>)> {
    let res = state.store.get_device(&gid, &did).await.map_err(to_err)?;
    match res {
        Some(item) => Ok(Json(item)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "device not found".to_string(),
            }),
        )),
    }
}

async fn post_device_seen(
    State(state): State<AppState>,
    Json(body): Json<DeviceSeenEvent>,
) -> Result<Json<store::Device>, (StatusCode, Json<ErrorResponse>)> {
    let stuff = if body.meta.is_null() { json!({}) } else { body.meta };
    state.store.upsert_device_seen(&body.gateway_id, &body.device_id, stuff)
        .await.map(Json).map_err(to_err)
}

async fn post_gateway_heartbeat(
    State(state): State<AppState>,
    Json(body): Json<GatewayHeartbeatEvent>,
) -> Result<Json<Gateway>, (StatusCode, Json<ErrorResponse>)> {
    let v = if body.meta.is_null() {
        json!({})
    } else {
        body.meta
    };
    state
        .store
        .upsert_gateway_seen(&body.gateway_id, v)
        .await
        .map(Json)
        .map_err(to_err)
}

// i keep going back and forth on whether to just use a middleware for auth
// but this is simpler for now and the prof said its fine
fn get_bearer_token(hdrs: &HeaderMap) -> Option<String> {
    let raw = hdrs.get("authorization").and_then(|v| v.to_str().ok())?.trim().to_string();
    let val = raw.strip_prefix("Bearer ").or_else(|| raw.strip_prefix("bearer "))?;
    let val = val.trim();
    if val.is_empty() { None } else { Some(val.to_string()) }
}

fn require_admin(hdrs: &HeaderMap, tok: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if tok.trim().is_empty() {
        return Ok(()); // no token configured = no auth needed
    }
    let t = hdrs.get("x-registry-admin-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| get_bearer_token(hdrs));
    if t.as_deref() == Some(tok) {
        return Ok(());
    }
    Err((
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse { error: "admin authorization required".to_string() }),
    ))
}

async fn require_gateway_token(
    st: &AppState,
    hdrs: &HeaderMap,
    gid: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let t = hdrs.get("x-gateway-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| get_bearer_token(hdrs));
    let Some(t) = t else {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse { error: "gateway authorization required".to_string() }),
        ));
    };
    let ok = st.store.verify_gateway_access_token(gid, &t).await.map_err(to_err)?;
    if ok { Ok(()) } else {
        Err((StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: "invalid gateway token".to_string() })))
    }
}

#[derive(Debug, Deserialize)]
struct GatewayOnboardingRequest {
    gateway_id: String,
    #[serde(default)]
    meta: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct GatewayOnboardingResponse {
    gateway: Gateway,
    enrollment_token: String,
}

async fn request_gateway_onboarding(
    State(state): State<AppState>,
    Json(body): Json<GatewayOnboardingRequest>,
) -> Result<Json<GatewayOnboardingResponse>, (StatusCode, Json<ErrorResponse>)> {
    let m = if body.meta.is_null() { json!({}) } else { body.meta };
    let (gw, tok) = state
        .store
        .request_gateway_onboarding(&body.gateway_id, m)
        .await
        .map_err(to_err)?;
    Ok(Json(GatewayOnboardingResponse {
        gateway: gw,
        enrollment_token: tok,
    }))
}

async fn approve_gateway(
    State(state): State<AppState>,
    hdrs: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Gateway>, (StatusCode, Json<ErrorResponse>)> {
    require_admin(&hdrs, &state.admin_token)?;
    state.store.approve_gateway(&id).await.map(Json).map_err(to_err)
}

#[derive(Debug, Deserialize)]
struct ClaimGatewayTokenRequest {
    enrollment_token: String,
}

#[derive(Debug, Serialize)]
struct ClaimGatewayTokenResponse {
    access_token: String,
}

async fn claim_gateway_access_token(
    State(state): State<AppState>,
    Path(gid): Path<String>,
    Json(body): Json<ClaimGatewayTokenRequest>,
) -> Result<Json<ClaimGatewayTokenResponse>, (StatusCode, Json<ErrorResponse>)> {
    let ret = state
        .store
        .claim_gateway_access_token(&gid, &body.enrollment_token)
        .await
        .map_err(to_err)?;
    Ok(Json(ClaimGatewayTokenResponse { access_token: ret }))
}

#[derive(Debug, Serialize)]
struct RotateGatewayTokenResponse {
    access_token: String,
}

async fn rotate_gateway_access_token(
    State(state): State<AppState>,
    hdrs: HeaderMap,
    Path(gid): Path<String>,
) -> Result<Json<RotateGatewayTokenResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_gateway_token(&state, &hdrs, &gid).await?;
    // kinda redundant to extract again but require_gateway_token doesnt return the token
    let tmp = hdrs.get("x-gateway-token")
        .and_then(|v| v.to_str().ok()).map(|s| s.to_string())
        .or_else(|| get_bearer_token(&hdrs));
    let Some(tmp) = tmp else {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "gateway authorization required".to_string(),
            }),
        ));
    };
    let res = state.store.rotate_gateway_access_token(&gid, &tmp).await.map_err(to_err)?;
    Ok(Json(RotateGatewayTokenResponse { access_token: res }))
}

async fn revoke_gateway(
    State(state): State<AppState>,
    hdrs: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Gateway>, (StatusCode, Json<ErrorResponse>)> {
    require_admin(&hdrs, &state.admin_token)?;
    state.store.revoke_gateway(&id).await.map(Json).map_err(to_err)
}

async fn decommission_gateway(
    State(state): State<AppState>,
    hdrs: HeaderMap,
    Path(x): Path<String>,
) -> Result<Json<Gateway>, (StatusCode, Json<ErrorResponse>)> {
    require_admin(&hdrs, &state.admin_token)?;
    state
        .store
        .decommission_gateway(&x)
        .await
        .map(Json)
        .map_err(to_err)
}

#[derive(Debug, Deserialize)]
struct RegisterDeviceRequest {
    device_id: String,
    #[serde(default)]
    meta: serde_json::Value,
}

async fn register_device(
    State(state): State<AppState>,
    hdrs: HeaderMap,
    Path(gid): Path<String>,
    Json(body): Json<RegisterDeviceRequest>,
) -> Result<Json<store::Device>, (StatusCode, Json<ErrorResponse>)> {
    require_gateway_token(&state, &hdrs, &gid).await?;
    let m = if body.meta.is_null() { json!({}) } else { body.meta };
    state.store.register_device(&gid, &body.device_id, m).await.map(Json).map_err(to_err)
}

async fn revoke_device(
    State(state): State<AppState>,
    hdrs: HeaderMap,
    Path((gid, did)): Path<(String, String)>,
) -> Result<Json<store::Device>, (StatusCode, Json<ErrorResponse>)> {
    require_admin(&hdrs, &state.admin_token)?;
    state.store.revoke_device(&gid, &did).await.map(Json).map_err(to_err)
}

async fn decommission_device(
    State(state): State<AppState>,
    hdrs: HeaderMap,
    Path((g, d)): Path<(String, String)>,
) -> Result<Json<store::Device>, (StatusCode, Json<ErrorResponse>)> {
    require_admin(&hdrs, &state.admin_token)?;
    state
        .store
        .decommission_device(&g, &d)
        .await
        .map(Json)
        .map_err(to_err)
}

// athena charges per byte scanned so we cap rows
const ALLOWED_COLUMNS: &[&str] = &[
    "device_id", "temperature", "voltage", "current", "power",
    "pressure", "compressor_speed", "frequency", "state_of_charge",
    "state", "ingest_ts",
];
const MAX_ROWS: u32 = 10_000;

#[derive(Debug, Deserialize)]
struct HistoryQueryRequest {
    date: String,
    #[serde(default)]
    device_ids: Vec<String>,
    #[serde(default)]
    columns: Vec<String>,
    #[serde(default = "default_limit")]
    limit: u32,
}

fn default_limit() -> u32 { 1000 }

#[derive(Debug, Serialize)]
struct HistoryQueryStartResponse {
    query_id: String,
}

#[derive(Debug, Serialize)]
struct HistoryQueryStatusResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    rows: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[cfg(feature = "athena")]
async fn start_history_query(
    State(state): State<AppState>,
    Json(body): Json<HistoryQueryRequest>,
) -> Result<Json<HistoryQueryStartResponse>, (StatusCode, Json<ErrorResponse>)> {
    let cfg2 = state.athena_config.as_ref().ok_or_else(|| {
        (StatusCode::NOT_IMPLEMENTED, Json(ErrorResponse {
            error: "historical queries not configured".to_string(),
        }))
    })?;
    let cl = state.athena_client.as_ref().ok_or_else(|| {
        (StatusCode::NOT_IMPLEMENTED, Json(ErrorResponse {
            error: "athena client not available".to_string(),
        }))
    })?;

    let dt = chrono::NaiveDate::parse_from_str(&body.date, "%Y-%m-%d").map_err(|_| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse {
            error: "invalid date format; expected YYYY-MM-DD".to_string(),
        }))
    })?;
    let yr = dt.format("%Y").to_string();
    let mo = dt.format("%m").to_string();
    let dy = dt.format("%d").to_string();

    let cols = if body.columns.is_empty() {
        "*".to_string()
    } else {
        for c in &body.columns {
            if !ALLOWED_COLUMNS.contains(&c.as_str()) {
                return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse {
                    error: format!("invalid column: {c}"),
                })));
            }
        }
        body.columns.join(", ")
    };

    let n = body.limit.min(MAX_ROWS);

    let mut whr = vec![
        format!("year = '{yr}'"),
        format!("month = '{mo}'"),
        format!("day = '{dy}'"),
    ];

    if !body.device_ids.is_empty() {
        // validate device ids to prevent sql injection... learned this the hard way in databases class
        for x in &body.device_ids {
            if !x.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse {
                    error: format!("invalid device_id: {x}"),
                })));
            }
        }
        let tmp: String = body.device_ids
            .iter()
            .map(|x| format!("'{x}'"))
            .collect::<Vec<_>>()
            .join(", ");
        whr.push(format!("device_id IN ({tmp})"));
    }

    let q = format!(
        "SELECT {cols} FROM {db}.{table} WHERE {where} ORDER BY ingest_ts DESC LIMIT {n}",
        db = cfg2.database,
        table = cfg2.table,
        where = whr.join(" AND "),
    );

    let r = cl
        .start_query_execution()
        .query_string(&q)
        .work_group(&cfg2.workgroup)
        .send()
        .await
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
                error: format!("failed to start query: {e}"),
            }))
        })?;

    let qid = r.query_execution_id().unwrap_or_default().to_string();
    Ok(Json(HistoryQueryStartResponse { query_id: qid }))
}

#[cfg(feature = "athena")]
async fn get_history_query(
    State(state): State<AppState>,
    Path(qid): Path<String>,
) -> Result<Json<HistoryQueryStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let cl = state.athena_client.as_ref().ok_or_else(|| {
        (StatusCode::NOT_IMPLEMENTED, Json(ErrorResponse {
            error: "athena client not available".to_string(),
        }))
    })?;

    let res = cl
        .get_query_execution()
        .query_execution_id(&qid)
        .send()
        .await
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
                error: format!("failed to get query status: {e}"),
            }))
        })?;

    let s = res
        .query_execution()
        .and_then(|qe| qe.status())
        .and_then(|st| st.state())
        .map(|st| st.as_str().to_string())
        .unwrap_or_else(|| "UNKNOWN".to_string());

    if s != "SUCCEEDED" {
        let err = if s == "FAILED" {
            res
                .query_execution()
                .and_then(|qe| qe.status())
                .and_then(|st| st.state_change_reason())
                .map(|r| r.to_string())
        } else {
            None
        };
        return Ok(Json(HistoryQueryStatusResponse {
            status: s,
            rows: None,
            error: err,
        }));
    }

    let data = cl
        .get_query_results()
        .query_execution_id(&qid)
        .send()
        .await
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
                error: format!("failed to get query results: {e}"),
            }))
        })?;

    let rs = data.result_set();

    let names: Vec<String> = rs
        .and_then(|r| r.result_set_metadata())
        .map(|m| {
            m.column_info()
                .iter()
                .map(|ci| ci.name().to_string())
                .collect()
        })
        .unwrap_or_default();

    let items: Vec<serde_json::Value> = rs
        .map(|r| {
            r.rows()
                .iter()
                .skip(1) // first row is column headers apparently
                .map(|row| {
                    let mut obj = serde_json::Map::new();
                    for (idx, datum) in row.data().iter().enumerate() {
                        let k = names.get(idx).cloned().unwrap_or_else(|| format!("col_{idx}"));
                        let v = datum.var_char_value()
                            .map(|x| serde_json::Value::String(x.to_string()))
                            .unwrap_or(serde_json::Value::Null);
                        obj.insert(k, v);
                    }
                    serde_json::Value::Object(obj)
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Json(HistoryQueryStatusResponse {
        status: s,
        rows: Some(items),
        error: None,
    }))
}

#[cfg(not(feature = "athena"))]
async fn start_history_query(
    Json(_req): Json<HistoryQueryRequest>,
) -> Result<Json<HistoryQueryStartResponse>, (StatusCode, Json<ErrorResponse>)> {
    Err((StatusCode::NOT_IMPLEMENTED, Json(ErrorResponse {
        error: "historical queries require building with --features athena".to_string(),
    })))
}

#[cfg(not(feature = "athena"))]
async fn get_history_query(
    Path(_query_id): Path<String>,
) -> Result<Json<HistoryQueryStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    Err((StatusCode::NOT_IMPLEMENTED, Json(ErrorResponse {
        error: "historical queries require building with --features athena".to_string(),
    })))
}

fn to_err(e: StoreError) -> (StatusCode, Json<ErrorResponse>) {
    let (c, s) = match &e {
        StoreError::BadRequest(s) => (StatusCode::BAD_REQUEST, s.clone()),
        StoreError::Unavailable(s) => (StatusCode::SERVICE_UNAVAILABLE, s.clone()),
        StoreError::Internal(s) => (StatusCode::INTERNAL_SERVER_ERROR, s.clone()),
    };
    (c, Json(ErrorResponse { error: s }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "registry=info".to_string()))
        .init();

    let cfg = load_config();

    let addr = SocketAddr::new(cfg.bind_addr, cfg.port);

    // copied from the axum examples, match on the backend string
    let s: Arc<dyn RegistryStore> = match cfg.backend.as_str() {
        "sqlite" => Arc::new(store::sqlite::SqliteRegistryStore::new(&cfg.sqlite_path)?),
        "dynamodb" => {
            #[cfg(feature = "dynamodb")]
            {
                let tbl = env_string("REGISTRY_DDB_TABLE", "");
                if tbl.trim().is_empty() {
                    return Err("REGISTRY_DDB_TABLE is required when REGISTRY_BACKEND=dynamodb".into());
                }
                Arc::new(store::dynamodb::DynamoRegistryStore::new(tbl).await?)
            }
            #[cfg(not(feature = "dynamodb"))]
            {
                return Err("REGISTRY_BACKEND=dynamodb requires building with --features dynamodb".into());
            }
        }
        other => {
            return Err(format!(
                "invalid REGISTRY_BACKEND='{other}' (expected 'sqlite' or 'dynamodb')"
            ).into())
        }
    };

    if cfg.mqtt_enabled {
        let tmp = mqtt_ingest::MqttIngestConfig {
            host: cfg.mqtt_host.clone(),
            port: cfg.mqtt_port,
            device_seen_filter: cfg.mqtt_device_seen_filter.clone(),
            gateway_heartbeat_filter: cfg.mqtt_gateway_heartbeat_filter.clone(),
        };
        let s2 = s.clone();
        tokio::spawn(async move {
            mqtt_ingest::run_mqtt_ingest(tmp, s2).await;
        });
    }

    let acfg = AthenaConfig::from_env();
    #[cfg(feature = "athena")]
    let acl = if acfg.is_some() {
        let x = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        Some(aws_sdk_athena::Client::new(&x))
    } else {
        None
    };

    let st = AppState {
        store: s,
        admin_token: cfg.admin_token.clone(),
        #[cfg(feature = "athena")]
        athena_client: acl,
        athena_config: acfg,
    };

    // TODO: add CORS middleware, the cloud-desktop app needs it
    let app = Router::new()
        .route("/health", get(health))
        .route("/provisioning/gateways", post(request_gateway_onboarding))
        .route(
            "/provisioning/gateways/:gateway_id/approve",
            post(approve_gateway),
        )
        .route(
            "/provisioning/gateways/:gateway_id/claim",
            post(claim_gateway_access_token),
        )
        .route(
            "/provisioning/gateways/:gateway_id/rotate_access_token",
            post(rotate_gateway_access_token),
        )
        .route(
            "/provisioning/gateways/:gateway_id/revoke",
            post(revoke_gateway),
        )
        .route(
            "/provisioning/gateways/:gateway_id/decommission",
            post(decommission_gateway),
        )
        .route("/provisioning/gateways/:gateway_id/devices", post(register_device))
        .route(
            "/provisioning/gateways/:gateway_id/devices/:device_id/revoke",
            post(revoke_device),
        )
        .route(
            "/provisioning/gateways/:gateway_id/devices/:device_id/decommission",
            post(decommission_device),
        )
        .route("/gateways", get(list_gateways))
        .route("/gateways/:gateway_id", get(get_gateway))
        .route("/gateways/:gateway_id/devices", get(list_devices))
        .route("/gateways/:gateway_id/devices/:device_id", get(get_device))
        .route("/events/device_seen", post(post_device_seen))
        .route("/events/gateway_heartbeat", post(post_gateway_heartbeat))
        .route("/history/query", post(start_history_query))
        .route("/history/query/:query_id", get(get_history_query))
        .with_state(st);

    let _ = axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await;

    Ok(())
}
