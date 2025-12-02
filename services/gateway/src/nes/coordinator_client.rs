use crate::nes::schema::{generate_schema_dsl, NesSchema};
use std::sync::OnceLock;
use std::time::Duration;

// the coordinator runs on ECS Fargate behind a Tailscale VPN address, so its
// iP changes every time the ECS task restarts. All URLs come from VPN discovery
// or the gateway config, never hardcoded.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

static HTTP: OnceLock<reqwest::Client> = OnceLock::new();

fn http() -> &'static reqwest::Client {
    HTTP.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .expect("failed to build HTTP client -- TLS backend unavailable?")
    })
}

#[derive(Debug, thiserror::Error)]
pub enum CoordinatorError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Coordinator returned {status}: {body}")]
    ApiError { status: u16, body: String },
}

/// register a logical source (schema) with the coordinator. Returns true if
/// newly added, false if it already existed (in which case we try to update it).
pub async fn register_logical_source(
    coord_url: &str,
    schema: &NesSchema,
) -> Result<bool, CoordinatorError> {
    let dsl = generate_schema_dsl(schema);
    let url = format!("{}/v1/nes/sourceCatalog/addLogicalSource",
        coord_url.trim_end_matches('/'));

    let body = serde_json::json!({
        "logicalSourceName": schema.logical_source_name,
        "schema": dsl,
    });

    let resp = http().post(&url).json(&body).send().await?;
    let st = resp.status();
    let resp_body = resp.text().await?;

    if st.is_success() {
        tracing::info!(logical_source = %schema.logical_source_name,
            "Logical source registered successfully");
        Ok(true)
    } else if st.as_u16() == 400 && resp_body.contains("already exists") {
        // schema already registered, try to update it in case fields changed
        tracing::info!(logical_source = %schema.logical_source_name,
            "Logical source already exists, attempting schema update");
        let _ = update_logical_source(coord_url, schema).await;
        Ok(false)
    } else {
        Err(CoordinatorError::ApiError { status: st.as_u16(), body: resp_body })
    }
}

/// retry wrapper for `register_logical_source`. The coordinator can take 30+
/// seconds to come up on ECS Fargate cold starts, so we retry with exponential
/// backoff up to `max_attempts`.
pub async fn register_logical_source_with_retry(
    coord_url: &str,
    schema: &NesSchema,
    max_attempts: u32,
) -> Result<bool, CoordinatorError> {
    let mut backoff = Duration::from_secs(1);

    for attempt in 1..=max_attempts {
        match register_logical_source(coord_url, schema).await {
            Ok(added) => return Ok(added),
            Err(e) => {
                if attempt == max_attempts {
                    tracing::error!(attempt, max_attempts, error = %e,
                        "could not register logical source");
                    return Err(e);
                }
                tracing::warn!(attempt, max_attempts, error = %e,
                    backoff_secs = backoff.as_secs(),
                    "Coordinator not ready, retrying...");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
        }
    }
    unreachable!()
}

// two different topology formats because the NES REST API isn't consistent --
// sometimes it returns a flat list of nodes, sometimes a tree. Node ID 1 is
// always the coordinator itself, so we skip it when looking for workers.
#[cfg(test)]
fn find_worker_in_flat_topology(topo: &serde_json::Value) -> Option<u32> {
    let nodes = topo.get("nodes")?.as_array()?;
    for n in nodes {
        if let Some(id) = n.get("id").and_then(serde_json::Value::as_u64) {
            if id > 1 {
                #[allow(clippy::cast_possible_truncation)]
                return Some(id as u32);
            }
        }
    }
    None
}

#[cfg(test)]
fn find_worker_in_tree(node: &serde_json::Value) -> Option<u32> {
    if let Some(id) = node.get("id").and_then(serde_json::Value::as_u64) {
        if id > 1 {
            #[allow(clippy::cast_possible_truncation)]
            return Some(id as u32);
        }
    }
    if let Some(kids) = node.get("children").and_then(serde_json::Value::as_array) {
        for kid in kids {
            if let Some(id) = find_worker_in_tree(kid) { return Some(id); }
        }
    }
    None
}

/// look up our worker in the coordinator topology by IP address. Node ID 1
/// is always the coordinator, so we skip it. Returns None if the worker isn't
/// in the topology (evicted, not yet registered, etc.).
pub async fn find_worker_by_ip(
    coord_url: &str, worker_ip: &str,
) -> Result<Option<u32>, CoordinatorError> {
    let url = format!("{}/v1/nes/topology", coord_url.trim_end_matches('/'));
    let resp = http().get(&url).send().await?;
    let st = resp.status();
    let body = resp.text().await?;

    if !st.is_success() {
        return Err(CoordinatorError::ApiError { status: st.as_u16(), body });
    }

    let topo: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| CoordinatorError::ApiError {
            status: st.as_u16(), body: format!("bad topology json: {e}"),
        })?;

    if let Some(nodes) = topo.get("nodes").and_then(serde_json::Value::as_array) {
        for n in nodes {
            let ip = n.get("ip_address").and_then(serde_json::Value::as_str).unwrap_or("");
            let id = n.get("id").and_then(serde_json::Value::as_u64).unwrap_or(0);
            if ip == worker_ip && id > 1 {
                #[allow(clippy::cast_possible_truncation)]
                return Ok(Some(id as u32));
            }
        }
    }
    Ok(None)
}

/// stale physical source entry, these accumulate when workers crash and
/// the coordinator never cleans them up (upstream NES bug, reported but unfixed).
/// we saw 34 of these pile up during the Joinville field test.
#[allow(dead_code, reason = "used by stale-source cleanup code that is not yet wired into the main lifecycle")]
#[derive(Debug, Clone)]
pub struct StalePhysicalSource {
    pub physical_source_name: String,
    pub logical_source_name: String,
    pub node_id: u64,
}

/// find physical sources that reference topology nodes that no longer exist.
/// this is how we detect the stale entries left behind by crashed workers.
#[allow(dead_code, reason = "used by stale-source cleanup code that is not yet wired into the main lifecycle")]
pub async fn find_stale_physical_sources(
    coord_url: &str,
    ls_name: &str,
) -> Result<Vec<StalePhysicalSource>, CoordinatorError> {
    let base = coord_url.trim_end_matches('/');
    let cl = http();

    // grab the topology to know which nodes are actually alive
    let topo_body = cl.get(format!("{base}/v1/nes/topology"))
        .send().await?.text().await?;
    let topo: serde_json::Value = serde_json::from_str(&topo_body).unwrap_or_default();

    let mut active_ids = std::collections::HashSet::new();
    if let Some(nodes) = topo.get("nodes").and_then(serde_json::Value::as_array) {
        for n in nodes {
            if let Some(id) = n.get("id").and_then(serde_json::Value::as_u64) {
                active_ids.insert(id);
            }
        }
    }

    // now check which physical sources reference dead nodes
    let ps_resp = cl.get(format!(
        "{base}/v1/nes/sourceCatalog/allPhysicalSource?logicalSourceName={ls_name}"
    )).send().await?;

    if !ps_resp.status().is_success() { return Ok(Vec::new()); }

    let ps_body = ps_resp.text().await?;
    let ps_json: serde_json::Value = serde_json::from_str(&ps_body).unwrap_or_default();

    let mut stale = Vec::new();
    if let Some(sources) = ps_json.get("physicalSources").and_then(serde_json::Value::as_array) {
        for src in sources {
            let nid = src.get("nodeId").and_then(serde_json::Value::as_u64).unwrap_or(0);
            if nid > 0 && !active_ids.contains(&nid) {
                let ps_name = src.get("physicalSourceName")
                    .and_then(serde_json::Value::as_str).unwrap_or("").to_string();
                let ls = src.get("logicalSourceName")
                    .and_then(serde_json::Value::as_str).unwrap_or(ls_name).to_string();
                tracing::warn!(
                    logical_source = ls_name, physical_source = %ps_name,
                    stale_node_id = nid,
                    "Detected stale physical source on non-existent topology node"
                );
                stale.push(StalePhysicalSource {
                    physical_source_name: ps_name,
                    logical_source_name: ls,
                    node_id: nid,
                });
            }
        }
    }
    Ok(stale)
}

#[allow(dead_code, reason = "used by stale-source cleanup code that is not yet wired into the main lifecycle")]
pub async fn remove_physical_source(
    coord_url: &str, stale: &StalePhysicalSource,
) -> Result<bool, CoordinatorError> {
    let base = coord_url.trim_end_matches('/');
    let url = format!(
        "{base}/v1/nes/sourceCatalog/removePhysicalSource\
         ?physicalSourceName={}\
         &logicalSourceName={}\
         &workerId={}",
        stale.physical_source_name, stale.logical_source_name, stale.node_id,
    );

    let resp = http().delete(&url).send().await?;
    let st = resp.status();
    if st.is_success() {
        Ok(true)
    } else {
        let body = resp.text().await?;
        tracing::warn!(physical_source = %stale.physical_source_name,
            status = st.as_u16(), body = %body,
            "could not remove physical source");
        Ok(false)
    }
}

pub async fn remove_logical_source(
    coord_url: &str, ls_name: &str,
) -> Result<bool, CoordinatorError> {
    let url = format!(
        "{}/v1/nes/sourceCatalog/deleteLogicalSource?logicalSourceName={ls_name}",
        coord_url.trim_end_matches('/')
    );

    let resp = http().delete(&url).send().await?;
    let st = resp.status();

    if st.is_success() {
        tracing::info!(logical_source = ls_name, "Logical source removed");
        Ok(true)
    } else if st.as_u16() == 404 {
        Ok(false) // wasn't there, no big deal
    } else {
        let body = resp.text().await?;
        // log before returning, lost 2 hours debugging a 409 that only
        // showed up as "ApiError" in the lifecycle logs. Never again.
        tracing::warn!(logical_source = ls_name, status = st.as_u16(),
            body = %body, "deleteLogicalSource rejected");
        Err(CoordinatorError::ApiError { status: st.as_u16(), body })
    }
}

/// remove ALL physical sources for a given worker node. This is the nuclear
/// option, used when we know the worker is dead and want to clean up.
#[allow(dead_code, reason = "used by stale-source cleanup code that is not yet wired into the main lifecycle")]
pub async fn remove_all_physical_sources_by_worker(
    coord_url: &str, worker_id: u64,
) -> Result<bool, CoordinatorError> {
    let url = format!(
        "{}/v1/nes/sourceCatalog/removeAllPhysicalSourcesByWorker?workerId={worker_id}",
        coord_url.trim_end_matches('/')
    );

    let resp = http().delete(&url).send().await?;
    let st = resp.status();
    if st.is_success() {
        Ok(true)
    } else if st.as_u16() == 404 {
        // endpoint doesn't exist in older NES versions
        tracing::warn!(worker_id, "removeAllPhysicalSourcesByWorker endpoint not available");
        Ok(false)
    } else {
        let body = resp.text().await?;
        tracing::warn!(worker_id, status = st.as_u16(), body = %body, "could not nuke worker sources");
        Ok(false)
    }
}

pub async fn update_logical_source(
    coord_url: &str, schema: &NesSchema,
) -> Result<bool, CoordinatorError> {
    let dsl = generate_schema_dsl(schema);
    let url = format!("{}/v1/nes/sourceCatalog/updateLogicalSource",
        coord_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "logicalSourceName": schema.logical_source_name,
        "schema": dsl,
    });

    let resp = http().post(&url).json(&body).send().await?;
    let st = resp.status();
    if st.is_success() { return Ok(true); }
    if st.as_u16() == 404 { return Ok(false); }

    let body = resp.text().await?;
    tracing::warn!(logical_source = %schema.logical_source_name,
        status = st.as_u16(), body = %body, "could not update logical source schema");
    Ok(false)
}

pub async fn check_coordinator_health(
    coord_url: &str,
) -> Result<bool, CoordinatorError> {
    let url = format!("{}/v1/nes/connectivity/check", coord_url.trim_end_matches('/'));

    let resp = http().get(&url).timeout(Duration::from_secs(5)).send().await?;
    let ok = resp.status().is_success();
    if !ok {
        tracing::warn!(status = resp.status().as_u16(),
            "Coordinator health check returned non-success status");
    }
    Ok(ok)
}

// --- query catalog ---

#[derive(Debug, Clone)]
pub struct QueryEntry {
    pub query_id: u64,
    pub status: String,
    #[allow(dead_code, reason = "field populated from API response for stale query detection")]
    pub query_string: String,
}

/// fetch all registered queries from the coordinator. Used by the query
/// monitor to detect stuck OPTIMIZING queries and auto-stop them.
pub async fn fetch_all_queries(
    coord_url: &str,
) -> Result<Vec<QueryEntry>, CoordinatorError> {
    let url = format!("{}/v1/nes/queryCatalog/allRegisteredQueries",
        coord_url.trim_end_matches('/'));

    let resp = http().get(&url).send().await?;
    let st = resp.status();
    let body = resp.text().await?;

    if !st.is_success() {
        return Err(CoordinatorError::ApiError { status: st.as_u16(), body });
    }

    let parsed: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| CoordinatorError::ApiError {
            status: 200, body: format!("bad query catalog json: {e}"),
        })?;

    let mut out = Vec::new();
    if let Some(queries) = parsed.as_array() {
        for q in queries {
            let qid = q.get("queryId").and_then(serde_json::Value::as_u64).unwrap_or(0);
            let status = q.get("queryStatus")
                .or_else(|| q.get("status"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("UNKNOWN").to_string();
            let qs = q.get("queryString")
                .and_then(serde_json::Value::as_str).unwrap_or("").to_string();
            out.push(QueryEntry { query_id: qid, status, query_string: qs });
        }
    }
    Ok(out)
}

pub async fn stop_query(
    coord_url: &str, query_id: u64,
) -> Result<(), CoordinatorError> {
    let url = format!("{}/v1/nes/query/stop-query?queryId={query_id}",
        coord_url.trim_end_matches('/'));
    let resp = http().delete(&url).send().await?;
    let st = resp.status();
    if st.is_success() { return Ok(()); }
    let body = resp.text().await?;
    Err(CoordinatorError::ApiError { status: st.as_u16(), body })
}

/// stop all running queries that reference a given logical source. Called
/// during cleanup before re-registering a logical source to avoid orphaned
/// queries pointing at a deleted source.
#[allow(dead_code, reason = "used by stale-query cleanup code that is not yet wired into the main lifecycle")]
pub async fn stop_stale_queries(coord_url: &str, ls_name: &str) -> u32 {
    let queries = match fetch_all_queries(coord_url).await {
        Ok(q) => q,
        Err(e) => {
            tracing::warn!("could not fetch query catalog for stale cleanup: {e}");
            return 0;
        }
    };

    let pattern = format!("Query::from(\"{ls_name}\")");
    let stale: Vec<_> = queries.iter()
        .filter(|q| q.query_string.contains(&pattern)
            && q.status != "STOPPED" && q.status != "FAILED")
        .collect();

    if stale.is_empty() { return 0; }

    tracing::info!("Found {} stale query(ies) referencing '{}', stopping",
        stale.len(), ls_name);

    let mut stopped = 0u32;
    for e in &stale {
        // TODO: parallelize with join_all? probably not worth it for <10 queries
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            stop_query(coord_url, e.query_id),
        ).await;
        match result {
            Ok(Ok(())) => stopped += 1,
            Ok(Err(err)) => tracing::warn!(query_id = e.query_id, "stop query err: {err}"),
            Err(_) => tracing::warn!(query_id = e.query_id,
                "Timed out stopping stale query, skipping"),
        }
    }
    stopped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::schema::NesField;

    // verify that register_logical_source sends a well-formed request to the
    // coordinator REST API. We spin up a raw TCP server because mockito pulls
    // in too many dependencies and wiremock was flaky on CI.
    #[tokio::test]
    async fn register_sends_correct_request() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut buf = vec![0u8; 4096];
            let n = tokio::io::AsyncReadExt::read(&mut stream, &mut buf)
                .await.expect("read");
            let request = String::from_utf8_lossy(&buf[..n]).to_string();

            let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"success\":true}";
            tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
                .await.expect("write");
            request
        });

        let schema = NesSchema {
            logical_source_name: "telemetry_0x0007".to_string(),
            fields: vec![
                NesField { name: "DEVICE_ID".into(), nes_type: "TEXT".into() },
                NesField { name: "timestamp".into(), nes_type: "UINT64".into() },
            ],
        };

        let result = register_logical_source(
            &format!("http://127.0.0.1:{}", addr.port()), &schema,
        ).await;

        assert!(result.is_ok(), "should succeed: {:?}", result);
        assert!(result.unwrap(), "should return true (added)");

        let request = server.await.expect("server task");
        assert!(request.contains("telemetry_0x0007"), "should contain source name");
        assert!(request.contains("BasicType::CHAR"), "should contain CHAR type for TEXT");
        assert!(request.contains("BasicType::UINT64"), "should contain UINT64 type");
    }

    #[test]
    fn finds_worker_in_tree_topology() {
        let topo = serde_json::json!({
            "id": 1,
            "children": [{ "id": 2, "children": [] }]
        });
        assert_eq!(find_worker_in_tree(&topo), Some(2));
    }

    #[test]
    fn returns_none_when_no_workers_in_tree() {
        // tree with only the coordinator node (id=1)
        let topo = serde_json::json!({ "id": 1, "children": [] });
        assert_eq!(find_worker_in_tree(&topo), None);
    }

    #[test]
    fn finds_worker_in() {
        // flat topology format, this is what the /v1/nes/topology endpoint
        // returns most of the time, but sometimes it returns a tree (see above)
        let topo = serde_json::json!({
            "nodes": [
                { "id": 1, "ip_address": "10.0.0.1" },
                { "id": 3, "ip_address": "100.88.85.75" }
            ],
            "edges": [{ "source": 3, "target": 1 }]
        });
        assert_eq!(find_worker_in_flat_topology(&topo), Some(3));
    }
}
