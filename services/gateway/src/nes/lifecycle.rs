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

const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(10);
const MAX_REGISTER_ATTEMPTS: u32 = 10;
const STALE_NODE_TIMEOUT: Duration = Duration::from_secs(120);
// workers auto-recycle after 30 minutes to prevent stale query state
const WORKER_TTL: Duration = Duration::from_secs(30 * 60);

pub async fn run_lifecycle(
    wk_cfg: WorkerConfig,
    vpn: VpnEndpoints,
    state: SharedState,
    gw_id: String,
) {
    state.write().unwrap().nes_status = NesStatus::WaitingForDevice;
    log(&state, LogLevel::Info, "NES lifecycle: waiting for device descriptor");

    let d = loop {
        if let Some(x) = state.read().ok().and_then(|s| s.descriptor.clone()) {
            break x;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    };

    check_docker(&wk_cfg.image, &state).await;

    let mut sc = build_schema(&d, wk_cfg.max_schema_fields);
    sc.logical_source_name = format!("{}_{}", sc.logical_source_name, gw_id);
    log(&state, LogLevel::Info, format!(
        "NES lifecycle: schema built, {} fields, source '{}'",
        sc.fields.len(), sc.logical_source_name,
    ));

    let mut pnid: Option<u32> = None;

    // main loop - this took me forever to get right
    loop {
        let cu = wk_cfg.coordinator_rest_url.clone().unwrap_or_else(|| {
            let ip = resolve_to_ip(&vpn.coordinator_host);
            format!("http://{}:{}", ip, vpn.coordinator_rest_port)
        });

        state.write().unwrap().nes_status = NesStatus::RegisteringSchema;

        let cn = format!("nes-worker-{gw_id}");
        kill_gateway_nes_container(&cn).await;

        if let Some(old) = pnid {
            if !wait_for_eviction(&cu, old, &state).await {
                log(&state, LogLevel::Warn,
                    format!("NES lifecycle: stale node {old} still there, continuing"));
            }
        }

        // nuke old logical source
        match remove_logical_source(&cu, &sc.logical_source_name).await {
            Ok(true) => {
                log(&state, LogLevel::Info, "NES lifecycle: logical source deleted, re-registering");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Ok(false) | Err(_) => {}
        }

        if let Err(e) = register_logical_source_with_retry(
            &cu, &sc, MAX_REGISTER_ATTEMPTS,
        ).await {
            let s = format!("NES lifecycle: schema registration failed: {e}");
            state.write().unwrap().nes_status = NesStatus::Error(s.clone());
            log(&state, LogLevel::Error, &s);
            tokio::time::sleep(Duration::from_secs(30)).await;
            continue;
        }

        log(&state, LogLevel::Info, "NES lifecycle: schema registered successfully");
        state.write().unwrap().nes_status = NesStatus::WorkerStarting;
        let mut rc = wk_cfg.clone();
        rc.coordinator_host = resolve_to_ip(&vpn.coordinator_host);
        rc.local_worker_host.clone_from(&vpn.local_ip);
        rc.physical_source_name = format!("{}-{}", wk_cfg.physical_source_name, gw_id);

        let s2 = sc.clone();
        let st = state.clone();
        let g2 = gw_id.clone();
        let wh = tokio::spawn(async move {
            run_worker_manager(&rc, Some(&s2), st, &g2).await;
        });

        let (ev, nid) = run_health_monitor(&cu, &vpn.local_ip, &state).await;
        pnid = nid;

        wh.abort();

        let r = ev.unwrap_or_else(|| "unknown".to_string());
        state.write().unwrap().nes_status = NesStatus::Reconnecting { reason: r.clone() };
        log(&state, LogLevel::Warn, format!("NES lifecycle: restarting, {r}"));

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn wait_for_eviction(coord_url: &str, nid: u32, state: &SharedState) -> bool {
    let deadline = tokio::time::Instant::now() + STALE_NODE_TIMEOUT;
    let mut errs: u32 = 0;

    loop {
        match find_node(coord_url, nid).await {
            Ok(true) => {
                errs = 0;
                if tokio::time::Instant::now() >= deadline { return false; }
                log(state, LogLevel::Info, format!("waiting for stale node {nid} to be evicted"));
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Ok(false) => { return true; }
            Err(_e) => {
                errs += 1;
                if errs >= 3 { return false; }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn find_node(
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

async fn run_health_monitor(
    coord_url: &str, local_ip: &str, state: &SharedState,
) -> (Option<String>, Option<u32>) {
    // give worker 15s to come up
    tokio::time::sleep(Duration::from_secs(15)).await;

    let mut fails: u32 = 0;
    let mut last_nid: Option<u32> = None;
    let started = tokio::time::Instant::now();

    loop {
        tokio::time::sleep(HEALTH_CHECK_INTERVAL).await;

        if started.elapsed() >= WORKER_TTL {
            stop_queries(coord_url, last_nid).await;
            return (Some(format!("TTL expired after {}m", WORKER_TTL.as_secs() / 60)), last_nid);
        }

        match check_coordinator_health(coord_url).await {
            Ok(true) => {
                match find_worker_by_ip(coord_url, local_ip).await {
                    Ok(Some(wid)) => {
                        fails = 0;
                        last_nid = Some(wid);
                        state.write().unwrap().nes_status = NesStatus::Connected { worker_id: wid };
                    }
                    Ok(None) => {
                        return (Some("worker not found in coordinator topology".to_string()), last_nid);
                    }
                    Err(_e) => { fails += 1; }
                }
            }
            Ok(false) => { fails += 1; }
            Err(_e) => { fails += 1; }
        }

        if fails >= 6 {
            return (Some(format!("coordinator unreachable for {fails} consecutive checks")), last_nid);
        }
    }
}

async fn stop_queries(coord_url: &str, worker_nid: Option<u32>) {
    use crate::nes::coordinator_client::{fetch_all_queries, stop_query};

    let queries = match fetch_all_queries(coord_url).await {
        Ok(q) => q,
        Err(_e) => return,
    };

    for q in &queries {
        if q.status == "RUNNING" || q.status == "OPTIMIZING" {
            let _ = stop_query(coord_url, q.query_id).await;
        }
    }

    let _ = worker_nid; // reserved for future per-worker query filtering
}

fn log(state: &SharedState, lvl: LogLevel, msg: impl Into<String>) {
    state.write().unwrap().push_log(lvl, msg.into());
}

async fn check_docker(img: &str, state: &SharedState) {
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
                let user = std::env::var("USER")
                    .or_else(|_| std::env::var("LOGNAME"))
                    .unwrap_or_else(|_| "root".to_string());
                let _ = tokio::process::Command::new("sudo")
                    .args(["usermod", "-aG", "docker", &user]).output().await;
                let _ = tokio::process::Command::new("sudo")
                    .args(["chmod", "666", "/var/run/docker.sock"]).output().await;
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                log(state, LogLevel::Error, format!("NES lifecycle: Docker install failed: {stderr}"));
                return;
            }
            Err(e) => {
                log(state, LogLevel::Error, format!("NES lifecycle: Docker install error: {e}"));
                return;
            }
        }
    }

    // check if image already pulled, docker pull is slow on RPi
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
            Err(_e) => {}
        }
    }
}

fn resolve_to_ip(host: &str) -> String {
    if host.parse::<std::net::IpAddr>().is_ok() {
        return host.to_string();
    }
    match (host, 0).to_socket_addrs() {
        Ok(mut v) => v.next().map_or_else(|| host.to_string(), |a| a.ip().to_string()),
        Err(_e) => host.to_string(),
    }
}
