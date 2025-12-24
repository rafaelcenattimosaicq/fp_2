use crate::config::WorkerConfig;
use crate::nes::coordinator_client::{
    check_coordinator_health, find_worker_by_ip,
    register_logical_source_with_retry,
    remove_logical_source,
};
use crate::nes::schema::build_schema;
use crate::nes::worker_manager::{kill_gateway_nes_container, run_worker_manager};
use crate::state::{LogLevel, NesStatus, SharedState};
use crate::vpn::VpnEndpoints;
use std::net::ToSocketAddrs;
use std::time::Duration;

const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(15);
const MAX_REGISTER_ATTEMPTS: u32 = 10;
// how long to wait for a stale worker node to disappear from the topology
// before giving up and proceeding. 120s is generous but the coordinator's
// heartbeat timeout is 60s and eviction can lag behind that.
const STALE_NODE_TIMEOUT: Duration = Duration::from_secs(120);

/// orchestrates the full NES worker lifecycle: wait for device descriptor,
/// build schema, register with coordinator, launch worker container, monitor
/// health, and restart on failure. This is the top-level task spawned from main.
pub async fn run_lifecycle(
    wk_cfg: WorkerConfig,
    vpn: VpnEndpoints,
    state: SharedState,
    gw_id: String,
) {
    state.write().unwrap().nes_status = NesStatus::WaitingForDevice;
    log(&state, LogLevel::Info, "NES lifecycle: waiting for device descriptor");

    // spin until the Modbus poller has fetched a descriptor from the cloud API
    // (or loaded one from the local fallback directory)
    let desc = loop {
        if let Some(d) = state.read().ok().and_then(|s| s.descriptor.clone()) {
            break d;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    };

    // make sure Docker is installed and the worker image is pulled before
    // we try to create a container, saves a confusing "docker: not found"
    // error message in the UI
    ensure_docker_ready(&wk_cfg.image, &state).await;

    let mut schema = build_schema(&desc, wk_cfg.max_schema_fields);
    // append gateway ID so each RPi gets a unique logical source name
    // in the coordinator's source catalog
    schema.logical_source_name = format!("{}_{}", schema.logical_source_name, gw_id);
    log(&state, LogLevel::Info, format!(
        "NES lifecycle: schema built, {} fields, source '{}'",
        schema.fields.len(), schema.logical_source_name,
    ));

    let mut prev_node_id: Option<u32> = None;

    loop {
        let coord_url = wk_cfg.coordinator_rest_url.clone().unwrap_or_else(|| {
            let ip = resolve_to_ip(&vpn.coordinator_host);
            format!("http://{}:{}", ip, vpn.coordinator_rest_port)
        });

        state.write().unwrap().nes_status = NesStatus::RegisteringSchema;

        let cname = format!("nes-worker-{gw_id}");
        kill_gateway_nes_container(&cname).await;

        // stale physical sources accumulate when the worker restarts but the
        // coordinator still has the old node's entries. If we re-register
        // before the old node is evicted, query placement sees two physical
        // sources for one logical source and picks the dead one. We saw 34
        // orphaned entries pile up during the Joinville field test, the
        // coordinator never cleans them up (upstream NES bug).
        if let Some(old_nid) = prev_node_id {
            if !wait_for_node_eviction(&coord_url, old_nid, &state).await {
                log(&state, LogLevel::Warn,
                    format!("NES lifecycle: stale node {old_nid} still there, continuing"));
            }
        }

        // nuke the old logical source so we can re-register with potentially
        // updated schema fields (descriptor can change between restarts if the
        // cloud API returns a newer version)
        match remove_logical_source(&coord_url, &schema.logical_source_name).await {
            Ok(true) => {
                log(&state, LogLevel::Info, "NES lifecycle: logical source deleted, re-registering");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Ok(false) | Err(_) => {} // didn't exist or API error, continue either way
            // FIXME: should we log Err? it's noisy but useful
        }

        if let Err(e) = register_logical_source_with_retry(
            &coord_url, &schema, MAX_REGISTER_ATTEMPTS,
        ).await {
            let err = format!("NES lifecycle: schema registration failed: {e}");
            tracing::warn!("{}", err);
            state.write().unwrap().nes_status = NesStatus::Error(err.clone());
            log(&state, LogLevel::Error, &err);
            tokio::time::sleep(Duration::from_secs(30)).await;
            continue;
        }

        log(&state, LogLevel::Info, "NES lifecycle: schema registered successfully");
        state.write().unwrap().nes_status = NesStatus::WorkerStarting;

        // resolve coordinator hostname to IP so the worker YAML has a numeric
        // address, NES worker doesn't do DNS resolution on its own
        let mut resolved_cfg = wk_cfg.clone();
        resolved_cfg.coordinator_host = resolve_to_ip(&vpn.coordinator_host);
        resolved_cfg.local_worker_host.clone_from(&vpn.local_ip);
        resolved_cfg.physical_source_name =
            format!("{}-{}", wk_cfg.physical_source_name, gw_id);

        let ws = schema.clone();
        let wst = state.clone();
        let wgid = gw_id.clone();
        let worker_handle = tokio::spawn(async move {
            run_worker_manager(&resolved_cfg, Some(&ws), wst, &wgid).await;
        });

        // monitor coordinator health and our worker's presence in the topology
        let (eviction_reason, last_nid) =
            run_health_monitor(&coord_url, &vpn.local_ip, &state).await;
        prev_node_id = last_nid;

        worker_handle.abort();

        let reason = eviction_reason.unwrap_or_else(|| "unknown".to_string());
        let msg = format!("NES lifecycle: restarting, {reason}");
        tracing::warn!("{}", msg);
        state.write().unwrap().nes_status = NesStatus::Reconnecting { reason: reason.clone() };
        log(&state, LogLevel::Warn, &msg);

        tokio::time::sleep(Duration::from_secs(5)).await;
    } // main loop
}

/// wait up to `STALE_NODE_TIMEOUT` for a node to disappear from the coordinator
/// topology. Returns true if the node was evicted, false on timeout or error.
async fn wait_for_node_eviction(coord_url: &str, nid: u32, state: &SharedState) -> bool {
    let deadline = tokio::time::Instant::now() + STALE_NODE_TIMEOUT;
    let mut consecutive_errs: u32 = 0;

    loop {
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!(node_id = nid, "Stale topology node still present after timeout");
            return false;
        }

        match find_node_in_topology(coord_url, nid).await {
            Ok(false) => {
                tracing::info!(node_id = nid, "Stale node evicted, proceeding");
                return true;
            }
            Ok(true) => {
                consecutive_errs = 0;
                let msg = format!("NES lifecycle: waiting for stale node {nid} to be evicted");
                tracing::info!("{}", msg);
                log(state, LogLevel::Info, &msg);
            }
            Err(e) => {
                consecutive_errs += 1;
                // don't spin for 120s against a coordinator that's genuinely
                // down, 3 consecutive failed fetches is enough to bail and
                // let the outer loop handle the reconnect
                if consecutive_errs >= 3 {
                    tracing::warn!(node_id = nid, error = %e,
                        "Coordinator unreachable during eviction wait, bailing");
                    return false;
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn find_node_in_topology(
    coord_url: &str, target_nid: u32,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!("{}/v1/nes/topology", coord_url.trim_end_matches('/'));
    let resp = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?
        .get(&url).send().await?;
    let body = resp.text().await?;
    let topo: serde_json::Value = serde_json::from_str(&body)?;

    if let Some(nodes) = topo.get("nodes").and_then(serde_json::Value::as_array) {
        for n in nodes {
            let id = n.get("id").and_then(serde_json::Value::as_u64).unwrap_or(0);
            #[allow(clippy::cast_possible_truncation)]
            if id as u32 == target_nid { return Ok(true); }
        }
    }
    Ok(false)
}

/// polls coordinator health + topology to check if our worker is still
/// registered. Returns when the worker disappears or the coordinator
/// becomes unreachable for 6 consecutive checks (~60s).
async fn run_health_monitor(
    coord_url: &str, local_ip: &str, state: &SharedState,
) -> (Option<String>, Option<u32>) {
    // give the worker 15s to start and register with the coordinator
    // before we start checking the topology
    tokio::time::sleep(Duration::from_secs(15)).await;

    let mut fails: u32 = 0;
    let mut last_nid: Option<u32> = None;

    loop {
        tokio::time::sleep(HEALTH_CHECK_INTERVAL).await;

        match check_coordinator_health(coord_url).await {
            Ok(true) => {
                match find_worker_by_ip(coord_url, local_ip).await {
                    Ok(Some(wid)) => {
                        fails = 0;
                        last_nid = Some(wid);
                        state.write().unwrap().nes_status = NesStatus::Connected { worker_id: wid };
                    }
                    Ok(None) => {
                        // our worker vanished from the topology, coordinator
                        // evicted it, probably because of a missed heartbeat
                        tracing::warn!(local_worker_ip = local_ip,
                            "Our worker not found in coordinator topology");
                        return (Some("worker not found in coordinator topology".to_string()), last_nid);
                    }
                    Err(e) => {
                        fails += 1;
                        tracing::warn!(error = %e, consecutive_failures = fails,
                            "Topology check failed");
                    }
                }
            }
            Ok(false) => {
                fails += 1;
                tracing::warn!(consecutive_failures = fails, "Coordinator health check returned unhealthy");
            }
            Err(e) => {
                fails += 1;
                tracing::warn!(error = %e, consecutive_failures = fails, "Coordinator unreachable");
            }
        }

        // 6 * 10s = ~60s of continuous failures before we give up and restart
        if fails >= 6 {
            let reason = format!("coordinator unreachable for {fails} consecutive checks");
            tracing::warn!("{}", reason);
            return (Some(reason), last_nid);
        }
    }
}

fn log(state: &SharedState, lvl: LogLevel, msg: impl Into<String>) {
    state.write().unwrap().push_log(lvl, msg.into());
}

/// make sure Docker is installed and the worker image is available locally.
/// on a fresh `RPi`, Docker might not be installed at all, we install it
/// via the convenience script and then pull the image.
async fn ensure_docker_ready(img: &str, state: &SharedState) {
    let docker_ok = tokio::process::Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output().await
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !docker_ok {
        log(state, LogLevel::Info, "NES lifecycle: Docker not found, installing");
        let install = tokio::process::Command::new("sh")
            .args(["-c", "curl -fsSL https://get.docker.com | sudo sh"])
            .output().await;

        match install {
            Ok(o) if o.status.success() => {
                log(state, LogLevel::Info, "NES lifecycle: Docker installed successfully");
                // add current user to docker group so we don't need sudo for every command
                let user = std::env::var("USER")
                    .or_else(|_| std::env::var("LOGNAME"))
                    .unwrap_or_else(|_| "root".to_string());
                let _ = tokio::process::Command::new("sudo")
                    .args(["usermod", "-aG", "docker", &user]).output().await;
                // also chmod the socket for the current session (usermod requires re-login)
                let _ = tokio::process::Command::new("sudo")
                    .args(["chmod", "666", "/var/run/docker.sock"]).output().await;
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                let msg = format!("NES lifecycle: Docker install failed: {stderr}");
                tracing::warn!("{}", msg);
                log(state, LogLevel::Error, &msg);
                return;
            }
            Err(e) => {
                log(state, LogLevel::Error, format!("NES lifecycle: Docker install error: {e}"));
                return;
            }
        }
    }

    // check if the image is already pulled, docker pull is slow on RPi
    // and we don't want to block the lifecycle for 5+ minutes on every restart
    let img_exists = tokio::process::Command::new("docker")
        .args(["image", "inspect", img]).output().await
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !img_exists {
        log(state, LogLevel::Info, format!("NES lifecycle: pulling worker image {img}"));
        match tokio::process::Command::new("docker").args(["pull", img]).output().await {
            Ok(o) if o.status.success() => {
                log(state, LogLevel::Info, "NES lifecycle: worker image pulled");
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                log(state, LogLevel::Warn, format!("NES lifecycle: image pull failed: {stderr}"));
            }
            Err(e) => tracing::warn!(error = %e, "Docker pull command failed"),
        }
    }
}

/// resolve a hostname to an IP address. Returns the hostname as-is if it's
/// already an IP or if DNS resolution fails. The NES worker binary can't do
/// dNS resolution on its own, so we have to feed it a numeric IP.
fn resolve_to_ip(host: &str) -> String {
    // fast path, already a numeric IP
    if host.parse::<std::net::IpAddr>().is_ok() {
        return host.to_string();
    }

    match (host, 0).to_socket_addrs() {
        Ok(mut addrs) => addrs.next().map_or_else(
            || {
                tracing::warn!(hostname = %host, "DNS returned no addresses, using hostname as-is");
                host.to_string()
            },
            |addr| addr.ip().to_string(),
        ),
        Err(e) => {
            tracing::warn!(hostname = %host, error = %e, "DNS resolution failed, using hostname as-is");
            host.to_string()
        }
    }
}
