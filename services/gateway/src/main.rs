#[allow(dead_code)] // fot the  descriptor types used only in modbus poller
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
#[allow(unused_imports)]
use std::collections::HashMap;

use eframe::egui;
use std::path::PathBuf;
use std::time::Duration;

fn is_headless() -> bool { std::env::args().any(|a| a == "--headless") }

fn main() {
    init_tracing();

    let x = load_config();
    let s = state::new_shared_state(x.gateway_id.clone());

    // sQLite history
    let db = storage::HistoryDb::open(&x.gateway_id).unwrap_or_else(|_e| {
        std::process::exit(1);
    });

    if is_headless() {
        run_headless(x, s, db);
        return;
    }

    let tmp = spawn_docker_setup(x.clone(), s.clone());
    let res = run_loading_ui(s.clone());

    let dg = tmp.join().ok().flatten();

    if res == ui::loading::LoadingOutcome::Cancelled {
        if let Some(g) = dg { g.cleanup_blocking(); }
        return;
    }

    ui::THEME_APPLIED.store(false, std::sync::atomic::Ordering::Relaxed);
    store_setup_info(&s, &x);

    let (tx, rx) = std::sync::mpsc::channel();

    let thing = x.gateway_id.clone();
    spawn_async_runtime(s.clone(), x, tx.clone(), rx, db.clone());
    run_ui(s, tx, db);

    if let Some(g) = dg {
        g.cleanup_blocking();
    }

    // kill the worker container
    let n = format!("nes-worker-{thing}");
    let r = tokio::runtime::Runtime::new().expect("cleanup runtime");
    r.block_on(nes::worker_manager::kill_gateway_nes_container(&n));
}

fn run_headless(cfg: config::GatewayConfig, shared: state::SharedState, history_db: storage::HistoryDb) {
    store_setup_info(&shared, &cfg);

    let (tx2, rx2) = std::sync::mpsc::channel();
    tx2
        .send(modbus::writer::BackgroundCommand::ConnectEmulated)
        .expect("channel should be open");

    let r = tokio::runtime::Runtime::new().expect("tokio runtime");
    r.block_on(async move {
        let tmp = std::sync::Arc::new(tokio::sync::Notify::new());
        let v = try_vpn(&cfg, &shared, &tmp).await;

        let mut mc = cfg.mqtt.clone();
        if let Some(ref ep) = v {
            patch_mqtt_broker(&mut mc.broker_url, ep);
        }

        // copia credenciais
        if let Ok(st) = shared.read() {
            if let Some((ref u, ref p)) = st.mqtt_credentials {
                mc.username = Some(u.clone());
                mc.password = Some(p.clone());
            }
        }

        let (cl, _rules) = mqtt::run_mqtt_loop(
            &mc, shared.clone(), tx2.clone(), cfg.gateway_id.clone(),
        );

        let s2 = shared.clone();
        let iv = Duration::from_millis(cfg.poll_interval_ms);
        let bd = cfg.serial.baud_rate;
        let sid = cfg.serial.slave_id;
        let eid = cfg.emulated_device_id;
        let url = cfg.devices_api_url.clone();
        let dd = cfg.devices_dir.clone();
        let rn = tmp.clone();
        tokio::spawn(async move {
            modbus::poller::run_poll_loop(
                bd, sid, iv, s2, rx2,
                rn, history_db, url, dd, eid,
            ).await;
        });

        let t = mc.topic.clone();
        let gw = cfg.gateway_id.clone();
        let q = mc.qos;
        let pm = cfg.poll_interval_ms;
        let mf = cfg.worker.as_ref().map_or(usize::MAX, |w| w.max_schema_fields);
        let s3 = shared.clone();
        tokio::spawn(async move {
            do_publish(&s3, &cl, &t, q, &gw, pm, mf).await;
        });

        let nh = if let Some(mut wc) = cfg.worker.clone() {
            if let Some(ref ep) = v {
                patch_worker_mqtt(&mut wc.mqtt_broker_url, ep);
            }

            let ep2 = v.unwrap_or_else(|| {
                vpn::VpnEndpoints {
                    local_ip: wc.local_worker_host.clone(),
                    coordinator_host: wc.coordinator_host.clone(),
                    coordinator_grpc_port: wc.coordinator_port,
                    coordinator_rest_port: 8081,
                    mqtt_broker_host: None,
                    mqtt_broker_port: 1883,
                }
            });

            let ru = wc.coordinator_rest_url.clone().unwrap_or_else(|| {
                format!("http://{}:{}", ep2.coordinator_host, ep2.coordinator_rest_port)
            });

            let s4 = shared.clone();
            let g2 = cfg.gateway_id.clone();
            let lh = tokio::spawn(async move {
                nes::lifecycle::run_lifecycle(wc, ep2, s4, g2).await;
            });

            let s5 = shared.clone();
            tokio::spawn(async move {
                nes::query_monitor::run_query_monitor(ru, s5).await;
            });

            Some(lh)
        } else {
            None
        };

        tokio::signal::ctrl_c().await.expect("ctrl-c listener");

        if let Some(h) = nh {
            h.abort();
            let _ = h.await;
        }

        let cn = format!("nes-worker-{}", cfg.gateway_id);
        nes::worker_manager::kill_gateway_nes_container(&cn).await;
    });
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gateway=info".parse().unwrap()),
        )
        .init();
}

fn load_config() -> config::GatewayConfig {
    let p = std::env::args()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .unwrap_or_else(|| "gateway.yaml".into());

    config::load_config(&PathBuf::from(&p)).unwrap_or_else(|_e| {
        std::process::exit(1);
    })
}

fn store_setup_info(shared: &state::SharedState, cfg: &config::GatewayConfig) {
    let data = vpn::fingerprint::HardwareFingerprint::collect();

    let val = cfg.vpn.as_ref()
        .and_then(|v| v.pre_shared_secret.clone())
        .unwrap_or_default();
    // ${GATEWAY_SECRET} is the envsubst placeholder, if it's still there
    // the operator forgot to set the env var before starting the service
    let ok = !val.is_empty() && val != "${GATEWAY_SECRET}";

    let cp = std::env::args()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .unwrap_or_else(|| "gateway.yaml".into());

    let mut s = shared.write().expect("store_setup_info: state lock poisoned");
    s.fingerprint = Some(data);
    s.vpn_secret_configured = ok;
    s.vpn_secret = val;
    s.config_path = cp;
}

fn spawn_docker_setup(
    cfg: config::GatewayConfig,
    st: state::SharedState,
) -> std::thread::JoinHandle<Option<docker::DockerGuard>> {
    // pick the llock
    if let Ok(mut v) = st.write() {
        v.docker_status = state::DockerStatus::Starting;
        v.push_log(state::LogLevel::Info, "Starting Docker containers...");
    }

    std::thread::spawn(move || {
        let tmp = tokio::runtime::Runtime::new().expect("temp runtime for Docker setup");
        match tmp.block_on(docker::ensure_containers(&cfg, &st)) {
            Ok(ret) => {
                if let Ok(mut v) = st.write() {
                    v.docker_status = state::DockerStatus::Running;
                    v.push_log(state::LogLevel::Info, "All Docker containers started successfully");
                }
                Some(ret)
            }
            Err(e) => {
                if let Ok(mut v) = st.write() {
                    v.docker_status = state::DockerStatus::Error(e.to_string());
                    v.push_log(state::LogLevel::Error, format!("Docker setup failed: {e}"));
                }
                None
            }
        }
    })
}

fn run_loading_ui(state: state::SharedState) -> ui::loading::LoadingOutcome {
    let stuff = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 400.0])
            .with_resizable(true),
        ..Default::default()
    };

    let item = std::sync::Arc::new(
        std::sync::Mutex::new(ui::loading::LoadingOutcome::Cancelled),
    );
    let x2 = item.clone();

    eframe::run_native(
        "IoT Gateway \u{2014} Starting",
        stuff,
        Box::new(move |_cc| {
            Ok(Box::new(LoadingAppWrapper {
                inner: ui::loading::LoadingApp::new(state),
                outcome: x2,
            }))
        }),
    )
    .expect("loading wdindow should start");

    let tmp = item.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let ret = *tmp;
    drop(tmp);
    ret
}

// takes ownership of the App
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
        let r = tokio::runtime::Runtime::new().expect("tokio runtime");
        r.block_on(async move {
            let n2 = std::sync::Arc::new(tokio::sync::Notify::new());
            let ep = try_vpn(&cfg, &state, &n2).await;

            let mut mc = cfg.mqtt.clone();
            if let Some(ref v) = ep {
                patch_mqtt_broker(&mut mc.broker_url, v);
            }

            if let Ok(s) = state.read() {
                if let Some((ref u, ref p)) = s.mqtt_credentials {
                    mc.username = Some(u.clone());
                    mc.password = Some(p.clone());
                }
            }

            let (cl, _rules) = mqtt::run_mqtt_loop(
                &mc, state.clone(), cmd_tx, cfg.gateway_id.clone(),
            );

            let s2 = state.clone();
            let iv = Duration::from_millis(cfg.poll_interval_ms);
            let bd = cfg.serial.baud_rate;
            let sl = cfg.serial.slave_id;
            let eid = cfg.emulated_device_id;
            let da = cfg.devices_api_url.clone();
            let dd = cfg.devices_dir.clone();
            let rn = n2.clone();

            tokio::spawn(async move {
                modbus::poller::run_poll_loop(
                    bd, sl, iv, s2, cmd_rx,
                    rn, history_db, da, dd, eid,
                ).await;
            });

            let tp = mc.topic.clone();
            let gw = cfg.gateway_id.clone();
            let q = mc.qos;
            let pm = cfg.poll_interval_ms;
            let mf = cfg.worker.as_ref().map_or(usize::MAX, |w| w.max_schema_fields);
            let s3 = state.clone();
            tokio::spawn(async move {
                do_publish(&s3, &cl, &tp, q, &gw, pm, mf).await;
            });

            if let Some(mut wc) = cfg.worker.clone() {
                if let Some(ref v) = ep {
                    patch_worker_mqtt(&mut wc.mqtt_broker_url, v);
                }

                if let Some(stuff) = ep {
                    let ru = wc.coordinator_rest_url.clone().unwrap_or_else(|| {
                        format!(
                            "http://{}:{}",
                            stuff.coordinator_host, stuff.coordinator_rest_port,
                        )
                    });

                    let s4 = state.clone();
                    let g2 = cfg.gateway_id.clone();
                    tokio::spawn(async move {
                        nes::lifecycle::run_lifecycle(wc, stuff, s4, g2).await;
                    });

                    let s5 = state.clone();
                    tokio::spawn(async move {
                        nes::query_monitor::run_query_monitor(ru, s5).await;
                    });
                } else {
                }
            }

            let c2 = cfg.clone();
            let s6 = state.clone();
            tokio::spawn(async move { docker::health_check_loop(c2, s6).await; });

            std::future::pending::<()>().await;
        });
    });
}

// why does this need to be async
async fn try_vpn(
    cfg: &config::GatewayConfig,
    state: &state::SharedState,
    retry: &tokio::sync::Notify,
) -> Option<vpn::VpnEndpoints> {
    let vc = cfg.vpn.as_ref()?;

    loop {
        match vpn::connect_vpn(vc, &cfg.gateway_id, state).await {
            Ok(x) => return Some(x),
            Err(_e) => {
                retry.notified().await;
            }
        }
    }
}

async fn do_publish(
    state: &state::SharedState,
    mqtt_client: &rumqttc::AsyncClient,
    topic: &str,
    qos: u8,
    gw_id: &str,
    poll_ms: u64,
    max_schema_fields: usize,
) {
    const NES_PREFIX: &str = "telemetry/nes";
    let mut ns: Option<nes::schema::NesSchema> = None;

    loop {
        tokio::time::sleep(Duration::from_millis(poll_ms)).await;

        let (v, did, tmp) = match state.read() {
            Ok(s) => {
                let x = s.descriptor.as_ref()
                    .and_then(|d| d.device_description.as_ref())
                    .and_then(|dd| dd.device_id.clone())
                    .unwrap_or_else(|| "unknown".into());
                (s.register_values.clone(), x, s.descriptor.clone())
            }
            Err(_) => continue,
        };

        if v.is_empty() { continue; }

        if let Err(_e) = mqtt::publish_telemetry(
            mqtt_client, topic, qos, gw_id, &did, &v,
        ).await {
        }

        if ns.is_none() {
            if let Some(ref d) = tmp {
                let mut item = nes::schema::build_schema(d, max_schema_fields);
                item.logical_source_name =
                    format!("{}_{gw_id}", item.logical_source_name);
                ns = Some(item);
            }
        }

        if let Some(ref sc) = ns {
            let t = chrono::Utc::now().timestamp_millis();
            let buf = nes::csv_publisher::build_json_line(
                sc, gw_id, &did, t, &v,
            );
            let nt = format!("{NES_PREFIX}/{}", sc.logical_source_name);
            let _ = mqtt_client
                .publish(&nt, mqtt::qos_from_u8(qos), false, buf.as_bytes().to_vec())
                .await;
        }
    }
}

fn patch_mqtt_broker(url: &mut String, ep: &vpn::VpnEndpoints) {
    if let Some(ref h) = ep.mqtt_broker_host {
        *url = format!("mqtt://{}:{}", h, ep.mqtt_broker_port);
    }
}

fn patch_worker_mqtt(url: &mut String, ep: &vpn::VpnEndpoints) {
    if let Some(ref h) = ep.mqtt_broker_host {
        *url = format!("tcp://{}:{}", h, ep.mqtt_broker_port);
    }
}

fn run_ui(
    state: state::SharedState,
    cmd_tx: std::sync::mpsc::Sender<modbus::writer::BackgroundCommand>,
    history_db: storage::HistoryDb,
) {
    let cfg2 = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 560.0])
            .with_min_inner_size([360.0, 280.0])
            .with_maximized(true),
        ..Default::default()
    };

    eframe::run_native(
        "IoT Gateway",
        cfg2,
        Box::new(move |_cc| Ok(Box::new(ui::GatewayApp::new(state, cmd_tx, history_db)))),
    )
    .expect("eframe should start");
}
