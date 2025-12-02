#[allow(dead_code)] // descriptor types used only in modbus poller at the moment
mod config;
#[allow(dead_code)]
mod device_descriptor;
mod docker;
mod ble;
mod descriptor_lookup;
mod firmware;
mod modbus;
mod mqtt;
mod nes;
mod storage;
mod vpn;
mod state;
mod telemetry;
mod transport;
mod ui;

use eframe::egui;
use std::path::PathBuf;
use std::time::Duration;

fn is_headless() -> bool { std::env::args().any(|a| a == "--headless") }

fn main() {
    init_tracing();

    let cfg = load_config();
    let shared = state::new_shared_state(cfg.gateway_id.clone());

    // sQLite history DB, fails hard on purpose. If the SD card is
    // full or read-only we want to know immediately, not discover it
    // 6 hours later when someone checks Grafana.
    let history_db = storage::HistoryDb::open(&cfg.gateway_id).unwrap_or_else(|e| {
        eprintln!("could not open history database: {e}");
        std::process::exit(1);
    });

    if is_headless() {
        run_headless(cfg, shared, history_db);
        return;
    }

    let docker_handle = spawn_docker_setup(cfg.clone(), shared.clone());
    let loading_outcome = run_loading_ui(shared.clone());

    let docker_guard = docker_handle.join().ok().flatten();

    if loading_outcome == ui::loading::LoadingOutcome::Cancelled {
        if let Some(g) = docker_guard { g.cleanup_blocking(); }
        return;
    }

    ui::THEME_APPLIED.store(false, std::sync::atomic::Ordering::Relaxed);
    store_setup_info(&shared, &cfg);

    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();

    let gw_id = cfg.gateway_id.clone();
    spawn_async_runtime(shared.clone(), cfg, cmd_tx.clone(), cmd_rx, history_db.clone());
    run_ui(shared, cmd_tx, history_db);

    if let Some(g) = docker_guard {
        g.cleanup_blocking();
    }

    // kill the NES worker container, it runs outside the DockerGuard's list
    // because its lifecycle is managed by nes::lifecycle, not by docker.rs
    let cname = format!("nes-worker-{gw_id}");
    let rt = tokio::runtime::Runtime::new().expect("cleanup runtime");
    rt.block_on(nes::worker_manager::kill_gateway_nes_container(&cname));
}

fn run_headless(cfg: config::GatewayConfig, shared: state::SharedState, history_db: storage::HistoryDb) {
    tracing::info!("Starting in headless mode (emulated transport, no GUI)");
    store_setup_info(&shared, &cfg);

    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
    cmd_tx
        .send(modbus::writer::BackgroundCommand::ConnectEmulated)
        .expect("channel should be open");

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async move {
        let vpn_retry = std::sync::Arc::new(tokio::sync::Notify::new());
        let vpn_ep = establish_vpn(&cfg, &shared, &vpn_retry).await;

        let mut mqtt_cfg = cfg.mqtt.clone();
        if let Some(ref ep) = vpn_ep {
            patch_mqtt_broker(&mut mqtt_cfg.broker_url, ep);
        }

        // copy MQTT credentials that the Docker setup stored in shared state.
        // in headless mode the Mosquitto container is already running from
        // a previous `ensure_containers` call (or from systemd on the Pi).
        if let Ok(s) = shared.read() {
            if let Some((ref u, ref p)) = s.mqtt_credentials {
                mqtt_cfg.username = Some(u.clone());
                mqtt_cfg.password = Some(p.clone());
            }
        }

        let mqtt_client = mqtt::run_mqtt_loop(
            &mqtt_cfg, shared.clone(), cmd_tx.clone(), cfg.gateway_id.clone(),
        );

        let poll_st = shared.clone();
        let interval = Duration::from_millis(cfg.poll_interval_ms);
        let baud = cfg.serial.baud_rate;
        let slave = cfg.serial.slave_id;
        let emu_id = cfg.emulated_device_id;
        let dev_api = cfg.devices_api_url.clone();
        let dev_dir = cfg.devices_dir.clone();
        let retry_n = vpn_retry.clone();
        tokio::spawn(async move {
            modbus::poller::run_poll_loop(
                baud, slave, interval, poll_st, cmd_rx,
                retry_n, history_db, dev_api, dev_dir, emu_id,
            ).await;
        });

        let topic = mqtt_cfg.topic.clone();
        let gw = cfg.gateway_id.clone();
        let qos = mqtt_cfg.qos;
        let poll_ms = cfg.poll_interval_ms;
        let max_flds = cfg.worker.as_ref().map_or(usize::MAX, |w| w.max_schema_fields);
        let pub_state = shared.clone();
        tokio::spawn(async move {
            publish_loop(&pub_state, &mqtt_client, &topic, qos, &gw, poll_ms, max_flds).await;
        });

        let nes_handle = if let Some(mut wcfg) = cfg.worker.clone() {
            if let Some(ref ep) = vpn_ep {
                patch_worker_mqtt(&mut wcfg.mqtt_broker_url, ep);
            }

            let endpoints = vpn_ep.unwrap_or_else(|| {
                vpn::VpnEndpoints {
                    local_ip: wcfg.local_worker_host.clone(),
                    coordinator_host: wcfg.coordinator_host.clone(),
                    coordinator_grpc_port: wcfg.coordinator_port,
                    coordinator_rest_port: 8081,
                    mqtt_broker_host: None,
                    mqtt_broker_port: 1883,
                }
            });

            let rest_url = wcfg.coordinator_rest_url.clone().unwrap_or_else(|| {
                format!("http://{}:{}", endpoints.coordinator_host, endpoints.coordinator_rest_port)
            });

            let ns = shared.clone();
            let ngw = cfg.gateway_id.clone();
            let lh = tokio::spawn(async move {
                nes::lifecycle::run_lifecycle(wcfg, endpoints, ns, ngw).await;
            });

            let qs = shared.clone();
            tokio::spawn(async move {
                nes::query_monitor::run_query_monitor(rest_url, qs).await;
            });

            Some(lh)
        } else {
            None
        };

        tracing::info!("Headless gateway running, press Ctrl-C to stop");
        tokio::signal::ctrl_c().await.expect("ctrl-c listener");
        tracing::info!("Shutting down...");

        if let Some(h) = nes_handle {
            h.abort();
            let _ = h.await;
        }

        let cname = format!("nes-worker-{}", cfg.gateway_id);
        tracing::info!(cname, "Stopping NES worker container");
        nes::worker_manager::kill_gateway_nes_container(&cname).await;
        tracing::info!("NES worker container stopped, exiting");
    });
}

fn init_tracing() {
    // gATEWAY_LOG=debug overrides for field debugging, otherwise info level
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gateway=info".parse().expect("valid filter")),
        )
        .init();
}

fn load_config() -> config::GatewayConfig {
    let path = std::env::args()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .unwrap_or_else(|| "gateway.yaml".into());

    config::load_config(&PathBuf::from(&path)).unwrap_or_else(|e| {
        eprintln!("could not load config from {path}: {e}");
        std::process::exit(1);
    })
}

fn store_setup_info(shared: &state::SharedState, cfg: &config::GatewayConfig) {
    let fp = vpn::fingerprint::HardwareFingerprint::collect();

    let secret = cfg.vpn.as_ref()
        .and_then(|v| v.pre_shared_secret.clone())
        .unwrap_or_default();
    // ${GATEWAY_SECRET} is the envsubst placeholder, if it's still there
    // the operator forgot to set the env var before starting the service
    let secret_ok = !secret.is_empty() && secret != "${GATEWAY_SECRET}";

    let cfg_path = std::env::args()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .unwrap_or_else(|| "gateway.yaml".into());

    let mut s = shared.write().expect("store_setup_info: state lock poisoned");
    s.fingerprint = Some(fp);
    s.vpn_secret_configured = secret_ok;
    s.vpn_secret = secret;
    s.config_path = cfg_path;
}

fn spawn_docker_setup(
    cfg: config::GatewayConfig,
    st: state::SharedState,
) -> std::thread::JoinHandle<Option<docker::DockerGuard>> {
    // grab the lock carefully here, if a background task panicked and poisoned
    // the RwLock (common after USB-serial yank) we still want to attempt Docker
    // setup because the UI thread may recover once the adapter is re-plugged.
    if let Ok(mut s) = st.write() {
        s.docker_status = state::DockerStatus::Starting;
        s.push_log(state::LogLevel::Info, "Starting Docker containers...");
    }

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("temp runtime for Docker setup");
        match rt.block_on(docker::ensure_containers(&cfg, &st)) {
            Ok(guard) => {
                if let Ok(mut s) = st.write() {
                    s.docker_status = state::DockerStatus::Running;
                    s.push_log(state::LogLevel::Info, "All Docker containers started successfully");
                }
                Some(guard)
            }
            Err(e) => {
                tracing::error!(error = %e, "Docker container setup failed");
                if let Ok(mut s) = st.write() {
                    s.docker_status = state::DockerStatus::Error(e.to_string());
                    s.push_log(state::LogLevel::Error, format!("Docker setup failed: {e}"));
                }
                None
            }
        }
    })
}

/// show the loading splash while Docker containers come up.
/// `eframe::run_native` consumes the App so we smuggle the outcome out
/// via Arc<Mutex>, the `LoadingAppWrapper` below exists for this reason.
fn run_loading_ui(state: state::SharedState) -> ui::loading::LoadingOutcome {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 400.0])
            .with_resizable(true),
        ..Default::default()
    };

    let outcome = std::sync::Arc::new(
        std::sync::Mutex::new(ui::loading::LoadingOutcome::Cancelled),
    );
    let out2 = outcome.clone();

    eframe::run_native(
        "IoT Gateway \u{2014} Starting",
        opts,
        Box::new(move |_cc| {
            Ok(Box::new(LoadingAppWrapper {
                inner: ui::loading::LoadingApp::new(state),
                outcome: out2,
            }))
        }),
    )
    .expect("loading window should start");

    let guard = outcome.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let result = *guard;
    drop(guard);
    result
}

// eframe takes ownership of the App, we can't read the outcome back after
// the window closes. This wrapper smuggles it out through Arc<Mutex>.
// it's ugly but eframe doesn't give us a better hook.
struct LoadingAppWrapper {
    inner: ui::loading::LoadingApp,
    outcome: std::sync::Arc<std::sync::Mutex<ui::loading::LoadingOutcome>>,
}

impl eframe::App for LoadingAppWrapper {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.inner.update(ctx, frame);
        if let Some(o) = self.inner.outcome() {
            *self.outcome.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = o;
        }
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.inner.on_exit(gl);
        // double-write is intentional, on_exit fires after the last update
        if let Some(o) = self.inner.outcome() {
            *self.outcome.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = o;
        }
    }
}

fn spawn_async_runtime(
    state: state::SharedState,
    cfg: config::GatewayConfig,
    cmd_tx: std::sync::mpsc::Sender<modbus::writer::BackgroundCommand>,
    cmd_rx: std::sync::mpsc::Receiver<modbus::writer::BackgroundCommand>,
    history_db: storage::HistoryDb,
) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async move {
            let vpn_retry = std::sync::Arc::new(tokio::sync::Notify::new());
            let vpn_ep = establish_vpn(&cfg, &state, &vpn_retry).await;

            let mut mqtt_cfg = cfg.mqtt.clone();
            if let Some(ref ep) = vpn_ep {
                patch_mqtt_broker(&mut mqtt_cfg.broker_url, ep);
            }

            if let Ok(s) = state.read() {
                if let Some((ref u, ref p)) = s.mqtt_credentials {
                    mqtt_cfg.username = Some(u.clone());
                    mqtt_cfg.password = Some(p.clone());
                }
            }

            let mqtt_client = mqtt::run_mqtt_loop(
                &mqtt_cfg, state.clone(), cmd_tx, cfg.gateway_id.clone(),
            );

            let ps = state.clone();
            let interval = Duration::from_millis(cfg.poll_interval_ms);
            let baud = cfg.serial.baud_rate;
            let slave = cfg.serial.slave_id;
            let emu_id = cfg.emulated_device_id;
            let dev_api = cfg.devices_api_url.clone();
            let dev_dir = cfg.devices_dir.clone();
            let retry_n = vpn_retry.clone();

            tokio::spawn(async move {
                modbus::poller::run_poll_loop(
                    baud, slave, interval, ps, cmd_rx,
                    retry_n, history_db, dev_api, dev_dir, emu_id,
                ).await;
            });

            let topic = mqtt_cfg.topic.clone();
            let gw = cfg.gateway_id.clone();
            let qos = mqtt_cfg.qos;
            let poll_ms = cfg.poll_interval_ms;
            let max_flds = cfg.worker.as_ref().map_or(usize::MAX, |w| w.max_schema_fields);
            let pub_st = state.clone();
            tokio::spawn(async move {
                publish_loop(&pub_st, &mqtt_client, &topic, qos, &gw, poll_ms, max_flds).await;
            });

            if let Some(mut wcfg) = cfg.worker.clone() {
                if let Some(ref ep) = vpn_ep {
                    patch_worker_mqtt(&mut wcfg.mqtt_broker_url, ep);
                }

                if let Some(endpoints) = vpn_ep {
                    let rest_url = wcfg.coordinator_rest_url.clone().unwrap_or_else(|| {
                        format!(
                            "http://{}:{}",
                            endpoints.coordinator_host, endpoints.coordinator_rest_port,
                        )
                    });

                    let ns = state.clone();
                    let ngw = cfg.gateway_id.clone();
                    tokio::spawn(async move {
                        nes::lifecycle::run_lifecycle(wcfg, endpoints, ns, ngw).await;
                    });

                    let qs = state.clone();
                    tokio::spawn(async move {
                        nes::query_monitor::run_query_monitor(rest_url, qs).await;
                    });
                } else {
                    tracing::info!("NES worker configured but VPN not available, skipping lifecycle");
                }
            }

            let dc = cfg.clone();
            let ds = state.clone();
            tokio::spawn(async move { docker::health_check_loop(dc, ds).await; });

            std::future::pending::<()>().await;
        });
    });
}

async fn establish_vpn(
    cfg: &config::GatewayConfig,
    state: &state::SharedState,
    retry: &tokio::sync::Notify,
) -> Option<vpn::VpnEndpoints> {
    let vpn_cfg = cfg.vpn.as_ref()?;

    loop {
        match vpn::connect_vpn(vpn_cfg, &cfg.gateway_id, state).await {
            Ok(ep) => return Some(ep),
            Err(e) => {
                tracing::warn!("VPN connect failed: {e}, waiting for user retry");
                retry.notified().await;
                tracing::info!("VPN retry requested by user");
            }
        }
    }
}

async fn publish_loop(
    state: &state::SharedState,
    mqtt_client: &rumqttc::AsyncClient,
    topic: &str,
    qos: u8,
    gw_id: &str,
    poll_ms: u64,
    max_schema_fields: usize,
) {
    const NES_PREFIX: &str = "telemetry/nes";
    let mut nes_schema: Option<nes::schema::NesSchema> = None;

    loop {
        tokio::time::sleep(Duration::from_millis(poll_ms)).await;

        let (vals, dev_id, desc) = match state.read() {
            Ok(s) => {
                let did = s.descriptor.as_ref()
                    .and_then(|d| d.device_description.as_ref())
                    .and_then(|dd| dd.device_id.clone())
                    .unwrap_or_else(|| "unknown".into());
                (s.register_values.clone(), did, s.descriptor.clone())
            }
            Err(_) => continue,
        };

        if vals.is_empty() { continue; }

        if let Err(e) = mqtt::publish_telemetry(
            mqtt_client, topic, qos, gw_id, &dev_id, &vals,
        ).await {
            tracing::warn!("publish error: {e}");
        }

        // build NES schema once we have a descriptor. The schema is immutable
        // after creation, if the descriptor changes mid-run we'd need a
        // restart, but that hasn't happened in practice.
        if nes_schema.is_none() {
            if let Some(ref d) = desc {
                let mut schema = nes::schema::build_schema(d, max_schema_fields);
                schema.logical_source_name =
                    format!("{}_{gw_id}", schema.logical_source_name);
                tracing::info!(
                    fields = schema.fields.len(),
                    logical_source = %schema.logical_source_name,
                    "NES publisher: built schema from descriptor"
                );
                nes_schema = Some(schema);
            }
        }

        if let Some(ref schema) = nes_schema {
            let ts = chrono::Utc::now().timestamp_millis();
            let line = nes::csv_publisher::build_json_line(
                schema, gw_id, &dev_id, ts, &vals,
            );
            let nes_topic = format!("{NES_PREFIX}/{}", schema.logical_source_name);
            if let Err(e) = mqtt_client
                .publish(&nes_topic, mqtt::qos_from_u8(qos), false, line.as_bytes().to_vec())
                .await
            {
                tracing::warn!("NES JSON publish: {e}");
            }
        }
    }
}

fn patch_mqtt_broker(url: &mut String, ep: &vpn::VpnEndpoints) {
    if let Some(ref host) = ep.mqtt_broker_host {
        let new = format!("mqtt://{}:{}", host, ep.mqtt_broker_port);
        tracing::info!(old = %url, new = %new, "MQTT broker URL overridden by VPN discovery");
        *url = new;
    }
}

fn patch_worker_mqtt(url: &mut String, ep: &vpn::VpnEndpoints) {
    if let Some(ref host) = ep.mqtt_broker_host {
        let new = format!("tcp://{}:{}", host, ep.mqtt_broker_port);
        tracing::info!(old = %url, new = %new, "NES worker MQTT URL overridden by VPN");
        *url = new;
    }
}

fn run_ui(
    state: state::SharedState,
    cmd_tx: std::sync::mpsc::Sender<modbus::writer::BackgroundCommand>,
    history_db: storage::HistoryDb,
) {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 560.0])
            .with_min_inner_size([360.0, 280.0])
            .with_maximized(true),
        ..Default::default()
    };

    eframe::run_native(
        "IoT Gateway",
        opts,
        Box::new(move |_cc| Ok(Box::new(ui::GatewayApp::new(state, cmd_tx, history_db)))),
    )
    .expect("eframe should start");
}
