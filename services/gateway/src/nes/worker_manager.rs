use crate::config::WorkerConfig;
use crate::nes::schema::NesSchema;
use crate::state::{LogLevel, SharedState};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

// Backoff starts at 1s, doubles up to 30s. Jitter ceiling picked by trial
// and error: 1500ms was enough to stagger 8 RPis rebooted simultaneously
// during the Joinville field test without being so large that a single
// gateway waits too long on a quiet network.
const INIT_BACKOFF: Duration = Duration::from_secs(2);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const JITTER_CEIL_MS: u64 = 1500;
const PORT_WAIT_TIMEOUT: Duration = Duration::from_secs(15);

/// main entry point, runs the NES worker Docker container in a supervised
/// restart loop. Waits for the coordinator to come up before each launch
/// so we don't burn through backoff retries against a cold ECS Fargate task.
#[allow(clippy::too_many_lines, reason = "container launch logic is inherently sequential; splitting would hurt readability")]
pub async fn run_worker_manager(
    wk_cfg: &WorkerConfig,
    schema: Option<&NesSchema>,
    gw_state: SharedState,
    gw_id: &str,
) {
    use std::process::Command as StdCmd;
    let mut backoff = INIT_BACKOFF;
    // coordinator REST is gRPC port + 1 by convention in NES Nautilus
    let coord_rest = wk_cfg.coordinator_rest_url.clone().unwrap_or_else(|| {
        format!(
            "http://{}:{}",
            wk_cfg.coordinator_host,
            wk_cfg.coordinator_port.saturating_add(1)
        )
    });

    loop {
        // dump the YAML to a temp file so we can bind-mount it into the container
        let cfg_path = match dump_worker_yaml(wk_cfg, schema) {
            Ok(p) => p,
            Err(e) => {
                push_log(&gw_state, LogLevel::Error, format!("NES worker: config write failed: {e}"));
                tokio::time::sleep(backoff).await;
                backoff = bump_backoff(backoff);
                continue;
            }
        };

        poll_coordinator_ready(&coord_rest, &gw_state).await;

        // --- spawn the container and tail its logs until it exits ---
        let cname = format!("nes-worker-{gw_id}");
        kill_gateway_nes_container(&cname).await;

        if !wait_ports_available(wk_cfg.rpc_port, wk_cfg.data_port, &gw_state).await {
            push_log(
                &gw_state, LogLevel::Error,
                format!("NES worker: ports {}/{} still busy, launching anyway", wk_cfg.rpc_port, wk_cfg.data_port),
            );
        }

        // on real Linux (RPi) we use host networking so the worker can bind to
        // the Tailscale IP directly. On macOS Docker Desktop the Tailscale IP
        // doesn't exist inside the container so we use bridge + port mapping +
        // bind_any.so LD_PRELOAD shim that intercepts bind() and replaces
        // the Tailscale IP with 0.0.0.0.
        let host_net = cfg!(target_os = "linux")
            || std::env::var("NES_FORCE_HOST_NETWORK").is_ok_and(|v| v == "1");

        push_log(&gw_state, LogLevel::Info, format!(
            "NES worker: starting {} (coord {}:{}, net={})",
            wk_cfg.image, wk_cfg.coordinator_host, wk_cfg.coordinator_port,
            if host_net { "host" } else { "bridge" },
        ));
        let container_cfg = "/tmp/nes-worker.yaml";

        let rpc_map = format!("{}:{}", wk_cfg.rpc_port, wk_cfg.rpc_port);
        let data_map = format!("{}:{}", wk_cfg.data_port, wk_cfg.data_port);
        let vol = format!("{cfg_path}:{container_cfg}:ro");
        let cfg_flag = format!("--configPath={container_cfg}");

        let bind_so = locate_shim("bind_any.so");
        let bind_mount = bind_so.as_ref().map(|p| format!("{p}:/opt/bind_any.so:ro"));

        // gRPC C core on ARM64 does an epoll_wait(timeout=0) busy-loop that pins
        // a CPU core at 100%. Found this the hard way when a Pi4 at the Joinville
        // plant thermal-throttled after 20 minutes. The epoll_timeout.so shim
        // patches epoll_wait to use a minimum 1ms timeout.
        let epoll_so = locate_shim("epoll_timeout.so");

        let t0 = std::time::Instant::now();
        if host_net {
            let ld = "/opt/epoll_timeout.so";
            let out = StdCmd::new("docker")
                .args([
                    "create", "--name", &cname, "--network=host",
                    "-e", &format!("LD_PRELOAD={ld}"),
                    &wk_cfg.image, &wk_cfg.binary_path, &cfg_flag,
                ])
                .output();
            if let Err(e) = out {
                push_log(&gw_state, LogLevel::Error, format!("NES worker: docker create: {e}"));
                let _ = std::fs::remove_file(&cfg_path);
                tokio::time::sleep(backoff).await;
                backoff = bump_backoff(backoff);
                continue;
            }

            // host networking + some docker versions don't support -v at create
            // time, so we docker-cp the config into the stopped container
            let cp_dst = format!("{cname}:{container_cfg}");
            let _ = StdCmd::new("docker").args(["cp", &cfg_path, &cp_dst]).output();

            if let Some(ref shim) = epoll_so {
                let shim_dst = format!("{cname}:/opt/epoll_timeout.so");
                let _ = StdCmd::new("docker").args(["cp", shim, &shim_dst]).output();
            }

        } else {
            // bridge mode, need explicit port mappings + bind_any shim
            let mut args: Vec<&str> = vec!["create", "--name", &cname];
            args.extend_from_slice(&["-p", &rpc_map, "-p", &data_map]);
            if let Some(ref m) = bind_mount {
                args.extend_from_slice(&["-v", m, "-e", "LD_PRELOAD=/opt/bind_any.so"]);
            }
            args.extend_from_slice(&["-v", &vol, &wk_cfg.image, &wk_cfg.binary_path, &cfg_flag]);
            let _ = StdCmd::new("docker").args(&args).output();
        }
        let _ = StdCmd::new("docker").args(["start", &cname]).output();

        // tail the container logs so the desktop UI can show NES output without
        // having to SSH into the Pi
        let mut child = match Command::new("docker")
            .args(["logs", "-f", &cname])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                push_log(&gw_state, LogLevel::Error, format!("NES worker: log follow spawn: {e}"));
                let _ = std::fs::remove_file(&cfg_path);
                tokio::time::sleep(backoff).await;
                backoff = bump_backoff(backoff);
                continue;
            }
        };

        push_log(&gw_state, LogLevel::Info, "NES worker: process started".to_string());

        if let Some(stdout) = child.stdout.take() {
            let st = gw_state.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(ln)) = lines.next_line().await {
                    tracing::info!(target: "nes_worker", "{}", ln);
                    push_log(&st, LogLevel::Info, format!("NES worker: {ln}"));
                }
            });
        }
        // stderr gets Warn level, most of it is NES debug spam but occasionally
        // there's a real SIGSEGV or gRPC error buried in there
        if let Some(stderr) = child.stderr.take() {
            let st = gw_state.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(ln)) = lines.next_line().await {
                    tracing::warn!(target: "nes_worker", "{}", ln);
                    push_log(&st, LogLevel::Warn, format!("NES worker (stderr): {ln}"));
                }
            });
        }

        match child.wait().await {
            Ok(exit) => {
                let msg = format!("NES worker: exited with {exit}");
                tracing::warn!("{}", msg);
                push_log(&gw_state, LogLevel::Warn, msg);
            }
            Err(e) => {
                tracing::error!("NES worker: wait error: {e}");
                push_log(&gw_state, LogLevel::Error, format!("NES worker: wait error: {e}"));
            }
        }

        let uptime = t0.elapsed();
        let _ = std::fs::remove_file(&cfg_path);

        // if worker lived >30s the coordinator was probably healthy at launch.
        // reset backoff so the next attempt doesn't wait needlessly after a
        // legitimate crash (OOM, SIGSEGV, etc.)
        if uptime > Duration::from_secs(30) {
            backoff = INIT_BACKOFF;
        }

        push_log(
            &gw_state, LogLevel::Info,
            format!("NES worker: restarting in {}s", backoff.as_secs()),
        );
        tokio::time::sleep(backoff).await;
        backoff = bump_backoff(backoff);
    }
}

fn dump_worker_yaml(wk_cfg: &WorkerConfig, schema: Option<&NesSchema>) -> Result<String, std::io::Error> {
    let yaml = generate_worker_yaml(wk_cfg, schema);
    let out = format!("/tmp/nes-worker-{}.yaml", std::process::id());
    std::fs::write(&out, yaml)?;
    Ok(out)
}

/// build the YAML config that the NES worker binary reads at startup.
/// coordinatorHost/Port, physical source, MQTT settings all come from the
/// gateway config; the logical source name can be overridden by the runtime
/// schema to include the gateway suffix (e.g. `telemetry_0x0007`).
pub fn generate_worker_yaml(wk_cfg: &WorkerConfig, schema: Option<&NesSchema>) -> String {
    let src_name = schema.map_or(
        wk_cfg.logical_source_name.as_str(),
        |s| s.logical_source_name.as_str(),
    );

    // topic includes source name so each gateway publishes to its own MQTT topic
    let mqtt_topic = format!("telemetry/nes/{src_name}");

    // numberOfSlots must be > 0 for query deployment. 65535 is the max and
    // we just use that because there's no real cost to having more slots
    // than queries. The coordinator assigns slots lazily.
    //
    // fixed rpc/data ports prevent stale health-check entries on the coordinator.
    // when ports are 0 (ephemeral), each worker restart picks a new port and the
    // coordinator never cleans up old entries, found 34 orphaned entries during
    // the Joinville incident. Using fixed ports means re-registrations reuse
    // the same address.
    format!(
        r#"logLevel: LOG_DEBUG
coordinatorHost: {coord_host}
localWorkerHost: {local_host}
coordinatorPort: {coord_port}
rpcPort: {rpc}
dataPort: {data}
numberOfSlots: 65535
physicalSources:
  - logicalSourceName: {src_name}
    physicalSourceName: {phys}
    # kAFKA_SOURCE hits a SIGSEGV in fillBuffer on ARM64 (NES 0.6.x).
    # switched to MQTT_SOURCE after losing 2 days debugging that crash.
    type: MQTT_SOURCE
    configuration:
      url: "{broker}"
      topic: "{mqtt_topic}"
      clientId: "nes-edge-worker"
      userName: "nes"
      qos: 1
      cleanSession: true
      inputFormat: JSON
      flushIntervalMS: 1000
      numberOfBuffersToProduce: 0
"#,
        coord_host = wk_cfg.coordinator_host,
        local_host = wk_cfg.local_worker_host,
        coord_port = wk_cfg.coordinator_port,
        rpc = wk_cfg.rpc_port,
        data = wk_cfg.data_port,
        phys = wk_cfg.physical_source_name,
        broker = wk_cfg.mqtt_broker_url,
    )
}

/// poll coordinator REST health until it returns 200. Blocks forever --
/// the outer restart loop handles timeouts via backoff.
async fn poll_coordinator_ready(rest_url: &str, st: &SharedState) {
    let url = format!("{rest_url}/v1/nes/connectivity/check");
    // unwrap_or_default is fine here, worst case we get a client with no TLS
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    loop {
        match http.get(&url).send().await {
            Ok(r) if r.status().is_success() => {
                push_log(st, LogLevel::Info, "NES worker: coordinator reachable".to_string());
                return;
            }
            Ok(r) => tracing::debug!(status = %r.status(), "coordinator not ready"),
            Err(e) => tracing::debug!(error = %e, "coordinator unreachable"),
        }
        push_log(st, LogLevel::Info,
            "NES worker: waiting for coordinator to become ready...".to_string());
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

fn bump_backoff(cur: Duration) -> Duration {
    let base = (cur * 2).min(MAX_BACKOFF);
    // jitter so N gateways rebooted simultaneously by an ECS deploy
    // don't all slam the coordinator REST API at the exact same instant
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::from(d.subsec_millis()) % JITTER_CEIL_MS);
    base + Duration::from_millis(jitter)
}

/// convenience wrapper, every callsite was doing the same
/// `state.write().unwrap().push_log(...)` dance and it was getting old.
fn push_log(st: &SharedState, lvl: LogLevel, msg: impl Into<String>) {
    let text = msg.into();
    if matches!(lvl, LogLevel::Error) {
        tracing::warn!("{text}");
    }
    // .unwrap() is intentional, if the lock is poisoned we want to crash
    // rather than silently lose log messages. The RwLock only poisons if the
    // modbus poller panics, which means the whole gateway is toast anyway.
    st.write().unwrap().push_log(lvl, text);
}

/// kill any leftover container from a previous run. This happens when the
/// gateway binary is `SIGKILLed` (e.g. systemd stop timeout exceeded) and
/// the container outlives the gateway process.
pub async fn kill_gateway_nes_container(cname: &str) {
    let found = Command::new("docker")
        .args(["ps", "-aq", "--filter", &format!("name=^{cname}$")])
        .output()
        .await
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false);

    if !found { return; }

    let _ = Command::new("docker").args(["stop", "-t", "2", cname]).output().await;
    let _ = Command::new("docker").args(["rm", "-f", cname]).output().await;
}

async fn wait_ports_available(rpc: u16, data: u16, st: &SharedState) -> bool {
    let deadline = tokio::time::Instant::now() + PORT_WAIT_TIMEOUT;
    loop {
        let rpc_free = !port_in_use(rpc).await;
        let data_free = !port_in_use(data).await;
        if rpc_free && data_free { return true; }
        if tokio::time::Instant::now() >= deadline { return false; }
        push_log(st, LogLevel::Info,
            format!("NES worker: waiting for ports {rpc}/{data} to be freed..."));
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// search for an `LD_PRELOAD` shim (.so) next to the gateway binary, then
/// fall back to cwd. Returns None if neither location has it.
fn locate_shim(name: &str) -> Option<String> {
    // check next to running binary first (works for .deb installs where
    // the shim lives in /usr/lib/gateway/nes-bind-override/)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("nes-bind-override").join(name);
            if p.exists() {
                return Some(p.to_string_lossy().to_string());
            }
        }
    }
    // cwd fallback for development layout (cargo run from services/gateway/)
    let cwd = std::path::Path::new("nes-bind-override").join(name);
    if cwd.exists() {
        if let Ok(abs) = cwd.canonicalize() {
            return Some(abs.to_string_lossy().to_string());
        }
    }
    tracing::warn!(%name, "LD_PRELOAD shim not found in any search path");
    None
}

async fn port_in_use(port: u16) -> bool {
    // ss is faster but only available on Linux
    if let Ok(o) = Command::new("ss")
        .args(["-tln", &format!("sport = :{port}")])
        .output().await
    {
        let out = String::from_utf8_lossy(&o.stdout);
        return out.lines().count() > 1; // header line + at least one listener
    }
    // macOS fallback, lsof is slower but always available
    if let Ok(o) = Command::new("lsof")
        .args(["-iTCP", "-sTCP:LISTEN", "-P", "-n"])
        .output().await
    {
        let needle = format!(":{port}");
        return String::from_utf8_lossy(&o.stdout).lines().any(|l| l.contains(&needle));
    }
    false // can't tell, assume free
}

#[cfg(test)]
mod tests {
    use super::*;

    // make sure the YAML we hand to the NES worker binary has all required
    // fields. A missing coordinatorPort once caused a 20-minute debug session
    // on a Pi that had no display attached, had to reflash the SD card
    // because the worker kept crashing in a tight loop filling /tmp.
    #[test]
    fn yaml_has_all_required_nes_fields() {
        let cfg = WorkerConfig {
            binary_path: "./nesWorker".to_string(),
            coordinator_host: "10.0.11.140".to_string(),
            coordinator_port: 8080,
            local_worker_host: "100.88.85.75".to_string(),
            logical_source_name: "telemetry".to_string(),
            physical_source_name: "edge-mqtt".to_string(),
            mqtt_broker_url: "tcp://localhost:1883".to_string(),
            mqtt_topic: "telemetry".to_string(),
            coordinator_rest_url: None,
            image: "ghcr.io/rafaelcenattimosaicq/nes-executable-image:latest".to_string(),
            max_schema_fields: 10,
            force_host_network: false,
            rpc_port: 40000,
            data_port: 40001,
        };

        let yaml = generate_worker_yaml(&cfg, None);
        assert!(yaml.contains("coordinatorHost: 10.0.11.140"));
        assert!(yaml.contains("localWorkerHost: 100.88.85.75"));
        assert!(yaml.contains("coordinatorPort: 8080"));
        assert!(yaml.contains("rpcPort: 40000"));
        assert!(yaml.contains("dataPort: 40001"));
        assert!(yaml.contains("logicalSourceName: telemetry"));
        assert!(yaml.contains("physicalSourceName: edge-mqtt"));
        // mQTT_SOURCE, NOT KAFKA_SOURCE, KAFKA one SIGSEGVs on ARM64
        assert!(yaml.contains("type: MQTT_SOURCE"));
        assert!(yaml.contains("url: \"tcp://localhost:1883\""));
        assert!(yaml.contains("topic: \"telemetry/nes/telemetry\""));
        assert!(yaml.contains("clientId: \"nes-edge-worker\""));
        assert!(yaml.contains("userName: \"nes\""));
        assert!(yaml.contains("qos: 1"));
        assert!(yaml.contains("cleanSession: true"));
        assert!(yaml.contains("inputFormat: JSON"));
        assert!(yaml.contains("flushIntervalMS: 1000"));
        assert!(yaml.contains("numberOfBuffersToProduce: 0"));
        assert!(yaml.contains("logLevel: LOG_DEBUG"));
    }

    // backoff roughly doubles each iteration. Jitter adds up to 1.5s, so
    // check a range rather than exact values.
    #[test]
    fn backoff_doubles_up_to_cap() {
        let ceil = Duration::from_millis(JITTER_CEIL_MS);
        let mut b = Duration::from_secs(1);

        b = bump_backoff(b);
        assert!(b >= Duration::from_secs(2) && b <= Duration::from_secs(2) + ceil);

        b = bump_backoff(b);
        assert!(b >= Duration::from_secs(4), "second bump should be ~4s+");

        for _ in 0..10 { b = bump_backoff(b); }
        assert!(b <= MAX_BACKOFF + ceil, "should never exceed max + jitter");
    }

    #[test]
    fn config_round_trips_through_temp_file() {
        let cfg = WorkerConfig {
            binary_path: "./nesWorker".to_string(),
            coordinator_host: "10.0.11.140".to_string(),
            coordinator_port: 8080,
            local_worker_host: "100.88.85.75".to_string(),
            logical_source_name: "telemetry".to_string(),
            physical_source_name: "the client-phys".to_string(),
            mqtt_broker_url: "tcp://localhost:1883".to_string(),
            mqtt_topic: "telemetry".to_string(),
            coordinator_rest_url: None,
            image: "ghcr.io/rafaelcenattimosaicq/nes-executable-image:latest".to_string(),
            max_schema_fields: 10,
            force_host_network: false,
            rpc_port: 40000,
            data_port: 40001,
        };

        let path = dump_worker_yaml(&cfg, None).expect("should write yaml");
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("coordinatorHost: 10.0.11.140"));
        assert!(contents.contains("localWorkerHost: 100.88.85.75"));
        assert!(contents.contains("type: MQTT_SOURCE"));
        assert!(contents.contains("url: \"tcp://localhost:1883\""));
        assert!(contents.contains("topic: \"telemetry/nes/telemetry\""));

        let _ = std::fs::remove_file(&path);
    }

    // when a NesSchema is provided its logical_source_name should override
    // the one from WorkerConfig. This is how the gateway_id suffix gets
    // propagated into the worker YAML so each RPi has a unique source name.
    #[test]
    fn schema_overrides_logical_source() {
        let cfg = WorkerConfig {
            binary_path: "./nesWorker".to_string(),
            coordinator_host: "10.0.11.140".to_string(),
            coordinator_port: 8080,
            local_worker_host: "100.88.85.75".to_string(),
            logical_source_name: "telemetry".to_string(),
            physical_source_name: "edge-mqtt".to_string(),
            mqtt_broker_url: "tcp://localhost:1883".to_string(),
            mqtt_topic: "telemetry".to_string(),
            coordinator_rest_url: None,
            image: "ghcr.io/rafaelcenattimosaicq/nes-executable-image:latest".to_string(),
            max_schema_fields: 10,
            force_host_network: false,
            rpc_port: 40000,
            data_port: 40001,
        };
        let schema = NesSchema {
            logical_source_name: "telemetry_0x0007".to_string(),
            fields: vec![],
        };

        let yaml = generate_worker_yaml(&cfg, Some(&schema));
        assert!(yaml.contains("logicalSourceName: telemetry_0x0007"));
        assert!(!yaml.contains("logicalSourceName: telemetry\n"));
        assert!(yaml.contains("topic: \"telemetry/nes/telemetry_0x0007\""));
    }
}
