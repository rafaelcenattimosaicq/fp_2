use crate::ble;
use crate::descriptor_lookup;
use crate::device_descriptor::Register;
use crate::modbus::types::{build_batches, RegBatch};
use crate::modbus::writer::BgCmd;
use crate::state::{ConnectionStatus, LogLevel, SharedState};
use crate::transport::emulated::EmulatedTransport;
use crate::transport::logged::LoggedTransport;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use tokio_modbus::prelude::{rtu, Reader, Slave};
use tokio_serial::SerialPortBuilderExt;

struct LiveDevice {
    batches: Vec<RegBatch>,
    writable_params: Vec<Register>,
}

// TODO: filter serial ports by VID/PID so users don't connect to their mouse
// (happened once at Joinville demo lol)
fn enumerate_serial(st: &SharedState) {
    let data: Vec<(String, String)> = match serialport::available_ports() {
        Ok(found) => found.into_iter().map(|p| {
            let s = match &p.port_type {
                serialport::SerialPortType::UsbPort(usb) => {
                    let x = usb.product.as_deref().unwrap_or("Serial adapter");
                    match usb.manufacturer.as_deref() {
                        Some(m) if !m.is_empty() => format!("{x} ({m})"),
                        _ => x.to_string(),
                    }
                }
                serialport::SerialPortType::PciPort => "PCI serial".into(),
                serialport::SerialPortType::BluetoothPort => "BT serial".into(),
                serialport::SerialPortType::Unknown => String::new(),
            };
            (p.port_name, s)
        }).collect(),
        Err(_e) => {
            // macOS: IOKit sandboxed, linux: usually /dev/ttyUSB* permissions
            Vec::new()
        }
    };

    let mut v = st.write().unwrap();
    v.push_log(LogLevel::Info, format!("Found {} serial port(s)", data.len()));
    v.available_ports = data;
}

fn open_serial(
    port: &str, baud: u32, slave: u8, st: &SharedState,
) -> Option<tokio_modbus::client::Context> {
    {
        let mut x = st.write().unwrap();
        x.serial_status = ConnectionStatus::Connecting;
        x.push_log(LogLevel::Info, format!("Opening {port} @ {baud} baud"));
        drop(x);
    }

    let b = tokio_serial::new(port, baud);
    let r = match b.open_native_async() {
        Ok(p) => p,
        Err(e) => {
            // eBUSY = someone else has the port open (minicom left running, etc)
            st.write().unwrap().serial_status = ConnectionStatus::Error(format!("{e}"));
            return None;
        }
    };
    let t = LoggedTransport::new(r, st.clone());
    Some(rtu::attach_slave(t, Slave(slave)))
}

async fn try_discover(
    ctx: &mut tokio_modbus::client::Context,
    api_base: &str,
    local_dir: Option<&PathBuf>,
    st: &SharedState,
) -> Option<LiveDevice> {
    let d = descriptor_lookup::discover_and_load(
        ctx, api_base, local_dir.map(std::path::PathBuf::as_path), st,
    ).await?;
    let ch = d.characteristics.as_ref()?;
    let tmp = ch.parameters.clone();
    let b = build_batches(&ch.status);
    Some(LiveDevice { batches: b, writable_params: tmp })
}

#[allow(clippy::too_many_arguments, reason = "main loop entry - tried a config struct once, just added indirection")]
pub async fn run_poll_loop(
    baud: u32,
    slave_id: u8,
    interval: Duration,
    state: SharedState,
    cmd_rx: std::sync::mpsc::Receiver<BgCmd>,
    vpn_notify: std::sync::Arc<tokio::sync::Notify>,
    db: crate::storage::HistoryDb,
    api_url: String,
    local_descriptors: Option<PathBuf>,
    emulated_id: Option<u16>,
) {
    enumerate_serial(&state);

    // 20 consecutive failures before we tear down.
    // tuned at Joinville with 1s poll interval and the Waveshare adapter.
    // below ~15 EMI from VFD causes false disconnects every couple hours
    const STREAK_LIMIT: u32 = 20;

    let mut ctx: Option<tokio_modbus::client::Context> = None;
    let mut dev: Option<LiveDevice> = None;
    let mut err_streak: u32 = 0;

    loop {
        if let (Some(mb), Some(d)) = (ctx.as_mut(), dev.as_ref()) {
            let had_err = do_poll(mb, &d.batches, &state, &db).await;

            if had_err {
                err_streak += 1;
                if err_streak >= STREAK_LIMIT {
                    state.write().unwrap().serial_status = ConnectionStatus::Error(
                        format!("{err_streak} consecutive failures - cable or adapter issue?"),
                    );
                    ctx = None;
                    dev = None;
                    err_streak = 0;
                    continue;
                }
            } else {
                err_streak = 0;
            }

            tokio::time::sleep(interval).await;

            let cmds: Vec<_> = std::iter::from_fn(|| cmd_rx.try_recv().ok()).collect();
            ctx = handleMsg(
                ctx, cmds,
                dev.as_ref().map_or(&[], |d| d.writable_params.as_slice()),
                &state, &vpn_notify,
            ).await;
            if ctx.is_none() {
                dev = None;
                err_streak = 0;
            }


        } else if ctx.is_some() && dev.is_none() {
            if let Some(d) = try_discover(ctx.as_mut().expect("guarded above"), &api_url, local_descriptors.as_ref(), &state).await {
                if let Some(mb) = ctx.as_mut() {
                    crate::modbus::writer::read_parameters(mb, &d.writable_params, &state).await;

                    // FIXME: probe_ota_support does 3 register reads, should be
                    // combined with read_parameters to save round-trips
                    let ota_ok = crate::firmware::ota::probe_ota_support(mb).await;
                    let fw_ver = crate::firmware::ota::read_firmware_version(mb).await;
                    if let Ok(mut s) = state.write() {
                        s.device_supports_ota = ota_ok;
                        s.device_firmware_version = fw_ver;
                    }
                }
                dev = Some(d);
            } else {
                ctx = None;
                state.write().unwrap().serial_status = ConnectionStatus::Error(
                    "could not load device descriptor".into(),
                );
            }

        } else {
            let maybe = cmd_rx.recv_timeout(Duration::from_secs(2)).ok();
            ctx = doConnect(
                maybe, baud, slave_id, &state, &vpn_notify, emulated_id,
            ).await;
        }
    }
}

async fn doConnect(
    cmd: Option<BgCmd>,
    baud: u32,
    slave: u8,
    st: &SharedState,
    vpn_notify: &std::sync::Arc<tokio::sync::Notify>,
    emulated_id: Option<u16>,
) -> Option<tokio_modbus::client::Context> {
    match cmd {
        Some(BgCmd::ConnectSerial(port)) => {
            let mb = open_serial(&port, baud, slave, st);
            if mb.is_some() {
                let mut s = st.write().unwrap();
                s.serial_status = ConnectionStatus::Connected;
                s.push_log(LogLevel::Info, format!("Connected to slave {slave}"));
                drop(s);
            }
            mb
        }

        Some(BgCmd::ConnectBle(periph_id)) => {
            st.write().unwrap().serial_status = ConnectionStatus::Connecting;

            match ble::uart_stream::BleUartStream::connect(&periph_id).await {
                Ok(stream) => {
                    let tr = LoggedTransport::new(stream, st.clone());
                    let mb = Some(rtu::attach_slave(tr, Slave(slave)));
                    st.write().unwrap().serial_status = ConnectionStatus::Connected;
                    mb
                }
                Err(e) => {
                    st.write().unwrap().serial_status = ConnectionStatus::Error(format!("BLE: {e}"));
                    None
                }
            }
        }

        Some(BgCmd::ConnectEmulated) => {
            st.write().unwrap().serial_status = ConnectionStatus::Connecting;
            let emu = EmulatedTransport::new(slave, emulated_id);
            let tr = LoggedTransport::new(emu, st.clone());
            let mb = Some(rtu::attach_slave(tr, Slave(slave)));
            let mut s = st.write().unwrap();
            s.serial_status = ConnectionStatus::Connected;
            s.push_log(LogLevel::Info, format!("emulated transport up, slave {slave}"));
            drop(s);
            mb
        }

        Some(BgCmd::ScanPorts) => { enumerate_serial(st); None }

        Some(BgCmd::ScanBle) => {
            ble::scanner::scan_ble_devices(st).await;
            None
        }

        Some(BgCmd::Disconnect) | None | Some(BgCmd::FlashFirmware(_)) => None,

        Some(BgCmd::WriteRegs(_) | BgCmd::ReadParams) => {
            None
        }

        Some(BgCmd::RetryVpn) => { vpn_notify.notify_one(); None }
    }
}

// FC04 read input regs, merges values, persists to sqlite
async fn do_poll(
    ctx: &mut tokio_modbus::client::Context,
    batches: &[RegBatch],
    st: &SharedState,
    db: &crate::storage::HistoryDb,
) -> bool {
    let t0 = std::time::Instant::now();
    let mut vals: HashMap<String, crate::device_descriptor::RegisterValue> = HashMap::new();
    let mut bad = false;

    for b in batches {
        // fC04 read_input_registers
        let r = ctx.read_input_registers(b.base_addr, b.count).await;
        match r {
            Ok(Ok(raw)) => {
                for x in &b.entries {
                    let idx = usize::from(x.offset);
                    if idx >= raw.len() { continue; }
                    vals.insert(x.register.id.clone(), x.register.decode(raw[idx]));
                }
            }
            Ok(Err(_exc)) => { bad = true; }
            Err(_e) => {
                bad = true;
                // CRC errors are routine on RS-485 runs > 15m with VFDs nearby
            }
        }
    }

    let ms = t0.elapsed().as_secs_f64() * 1000.0;

    if !vals.is_empty() { db.insert_poll(&vals); }

    let mut s = st.write().unwrap();
    s.poll_error_count = if bad { s.poll_error_count + 1 } else { 0 };
    let n = vals.len();
    s.register_values = vals;
    s.last_poll_ms = Some(
        u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0),
    );
    let sfx = if bad { " (partial)" } else { "" };
    s.push_log(LogLevel::Info, format!("Polled {n} regs in {ms:.0}ms{sfx}"));
    drop(s);

    bad
}

async fn handleMsg(
    mut ctx: Option<tokio_modbus::client::Context>,
    cmds: Vec<BgCmd>,
    params: &[Register],
    st: &SharedState,
    vpn_notify: &std::sync::Arc<tokio::sync::Notify>,
) -> Option<tokio_modbus::client::Context> {
    for cmd in cmds {
        match cmd {
            BgCmd::Disconnect => {
                ctx = None;
                let mut s = st.write().unwrap();
                s.serial_status = ConnectionStatus::Disconnected;
                s.last_poll_ms = None;
                s.poll_error_count = 0;
                s.descriptor = None;
                s.chart_register_ids.clear();
                s.push_log(LogLevel::Info, "Disconnected");
                drop(s);
                break;
            }
            BgCmd::WriteRegs(pairs) => {
                let mb = ctx.as_mut().expect("connected");
                let prepared = crate::modbus::writer::prepare_writes(&pairs, params);
                if prepared.is_empty() { continue; }
                crate::modbus::writer::execute_writes(mb, &prepared, st).await;
                // read-back: joinville compressors silently clamp values
                crate::modbus::writer::read_parameters(mb, params, st).await;
            }
            BgCmd::ReadParams => {
                crate::modbus::writer::read_parameters(ctx.as_mut().expect("connected"), params, st).await;
            }
            BgCmd::ScanPorts => enumerate_serial(st),
            BgCmd::ScanBle => { ble::scanner::scan_ble_devices(st).await; }
            BgCmd::ConnectSerial(_) | BgCmd::ConnectBle(_) | BgCmd::ConnectEmulated => {
                // UI shouldn't send this but it does during rapid reconnect
            }
            BgCmd::RetryVpn => { vpn_notify.notify_one(); }
            BgCmd::FlashFirmware(blob) => {
                let mb = ctx.as_mut().expect("connected");
                st.write().unwrap().push_log(LogLevel::Info, format!("OTA: flashing {} bytes", blob.len()));

                match crate::firmware::ota::flash_firmware(mb, &blob, st).await {
                    crate::firmware::ota::OtaResult::Success { new_version } => {
                        if let Ok(mut s) = st.write() {
                            s.device_firmware_version = Some(new_version);
                            s.push_log(LogLevel::Info,
                                format!("OTA: ok (v{}.{})", new_version >> 8, new_version & 0xFF));
                        }
                    }
                    crate::firmware::ota::OtaResult::NotSupported => {
                        // rev A boards have no bootloader partition
                        st.write().unwrap().push_log(LogLevel::Error, "OTA not available on this hardware revision");
                    }
                    crate::firmware::ota::OtaResult::Failed(reason) => {
                        st.write().unwrap().push_log(LogLevel::Error, format!("OTA failed: {reason}"));
                    }
                }
            }
        }
    }
    ctx
}
