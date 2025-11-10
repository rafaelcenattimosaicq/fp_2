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

/// lists serial ports visible to the OS and pushes them into shared state.
/// on the `RPi4` with the Waveshare USB-RS485-B hat it's always /dev/ttyUSB0.
/// fTDI adapters also show up as ttyUSB0 but with different VID/PID.
/// TODO: we should probably filter by VID/PID so users don't try to connect
/// to their mouse or keyboard by accident (happened once at Joinville demo)
fn enumerate_serial(st: &SharedState) {
    let ports: Vec<(String, String)> = match serialport::available_ports() {
        Ok(found) => found.into_iter().map(|p| {
            let lbl = match &p.port_type {
                serialport::SerialPortType::UsbPort(usb) => {
                    let prod = usb.product.as_deref().unwrap_or("Serial adapter");
                    // some adapters report manufacturer as empty string, not None
                    match usb.manufacturer.as_deref() {
                        Some(m) if !m.is_empty() => format!("{prod} ({m})"),
                        _ => prod.to_string(),
                    }
                }
                serialport::SerialPortType::PciPort => "PCI serial".into(),
                serialport::SerialPortType::BluetoothPort => "BT serial".into(),
                serialport::SerialPortType::Unknown => String::new(),
            };
            (p.port_name, lbl)
        }).collect(),
        Err(e) => {
            // on macOS this fails if IOKit is sandboxed. on linux it's usually
            // a permissions issue (/dev/ttyUSB* needs dialout group)
            tracing::warn!("port enumeration failed: {e}");
            Vec::new()
        }
    };

    let mut s = st.write().unwrap();
    s.push_log(LogLevel::Info, format!("Found {} serial port(s)", ports.len()));
    s.available_ports = ports;
}

/// opens the serial port and wraps it in a Modbus RTU context.
///
/// sets status to Connecting only, caller flips to Connected after descriptor
/// discovery works. We learned the hard way at Joinville: field techs saw a
/// brief "Connected" then "Error" when descriptor fetch failed over flaky 4G,
/// and they'd file tickets about intermittent connection. Took us 3 days to
/// figure out it was just the descriptor API timing out.
fn open_serial(
    port: &str,
    baud: u32,
    slave: u8,
    st: &SharedState,
) -> Option<tokio_modbus::client::Context> {
    {
        let mut s = st.write().unwrap();
        s.serial_status = ConnectionStatus::Connecting;
        s.push_log(LogLevel::Info, format!("Opening {port} @ {baud} baud"));
        drop(s);
    }

    let builder = tokio_serial::new(port, baud);
    let raw_port = match builder.open_native_async() {
        Ok(p) => p,
        Err(e) => {
            // eBUSY = someone else has the port open (minicom left running, etc)
            // eNOENT = port disappeared between enumeration and open (USB yank)
            eprintln!("serial open: {e}");
            st.write().unwrap().serial_status = ConnectionStatus::Error(format!("{e}"));
            return None;
        }
    };
    let transport = LoggedTransport::new(raw_port, st.clone());
    Some(rtu::attach_slave(transport, Slave(slave)))
}

async fn try_discover(
    ctx: &mut tokio_modbus::client::Context,
    api_base: &str,
    local_dir: Option<&PathBuf>,
    st: &SharedState,
) -> Option<LiveDevice> {
    let desc = descriptor_lookup::discover_and_load(
        ctx, api_base, local_dir.map(std::path::PathBuf::as_path), st,
    ).await?;

    let chars = desc.characteristics.as_ref()?;
    let wp = chars.parameters.clone();
    let batches = build_batches(&chars.status);

    // useful to see in prod logs when debugging "why is the poll so slow"
    tracing::info!(
        status_regs = chars.status.len(),
        batch_count = batches.len(),
        writable = wp.len(),
        "descriptor loaded, entering poll loop"
    );

    Some(LiveDevice { batches, writable_params: wp })
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

    // 20 consecutive failures before we tear down the connection.
    // tuned at Joinville with 1s poll interval and the Waveshare adapter.
    // below ~15 the EMI from VFD switching near the panel causes false
    // disconnects every couple hours. Above 30 a genuinely dead compressor
    // sits there for 30+ seconds showing stale data before we notice.
    const STREAK_LIMIT: u32 = 20;

    let mut ctx: Option<tokio_modbus::client::Context> = None;
    let mut dev: Option<LiveDevice> = None;
    let mut err_streak: u32 = 0;

    loop {
        // ---- actively polling ----
        if let (Some(mb), Some(d)) = (ctx.as_mut(), dev.as_ref()) {
            let had_err = do_poll_cycle(mb, &d.batches, &state, &db).await;

            if had_err {
                err_streak += 1;
                if err_streak >= STREAK_LIMIT {
                    tracing::error!(n = err_streak, "consecutive poll failures, tearing down connection");
                    state.write().unwrap().serial_status = ConnectionStatus::Error(
                        format!("{err_streak} consecutive failures - cable or adapter issue?"),
                    );
                    // nuke everything and go back to disconnected state
                    ctx = None;
                    dev = None;
                    err_streak = 0;
                    continue;
                }
            } else {
                err_streak = 0;
            }

            tokio::time::sleep(interval).await;

            // drain whatever commands piled up while we were sleeping
            let cmds: Vec<_> = std::iter::from_fn(|| cmd_rx.try_recv().ok()).collect();
            ctx = handle_while_connected(
                ctx,
                cmds,
                dev.as_ref().map_or(&[], |d| d.writable_params.as_slice()),
                &state,
                &vpn_notify,
            ).await;
            if ctx.is_none() {
                dev = None;
                err_streak = 0;
            }

        // ---- connected but still loading descriptor ----
        } else if ctx.is_some() && dev.is_none() {
            if let Some(d) = try_discover(ctx.as_mut().expect("guarded above"), &api_url, local_descriptors.as_ref(), &state).await {
                // read initial param values + check if OTA is available
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
                    tracing::info!(ota = ota_ok, fw = ?fw_ver, "OTA probe done");
                }
                dev = Some(d);
            } else {
                ctx = None;
                tracing::warn!("descriptor load failed, dropping serial context");
                state.write().unwrap().serial_status = ConnectionStatus::Error(
                    "could not load device descriptor".into(),
                );
            }

        // ---- fully disconnected, waiting for UI commands ----
        } else {
            // recv_timeout so we don't block forever if the UI crashes
            let maybe = cmd_rx.recv_timeout(Duration::from_secs(2)).ok();
            ctx = handle_while_disconnected(
                maybe, baud, slave_id, &state, &vpn_notify, emulated_id,
            ).await;
        }
    }
}

async fn handle_while_disconnected(
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

            // bLE UART (Nordic NUS) characteristic sometimes isn't ready right
            // after GAP connect completes. Retries are inside BleUartStream::connect
            // now. If it still fails it's usually because the the client controller is
            // bonded to someone else's phone and won't accept a new central.
            match ble::uart_stream::BleUartStream::connect(&periph_id).await {
                Ok(stream) => {
                    let tr = LoggedTransport::new(stream, st.clone());
                    let mb = Some(rtu::attach_slave(tr, Slave(slave)));
                    st.write().unwrap().serial_status = ConnectionStatus::Connected;
                    mb
                }
                Err(e) => {
                    tracing::error!("BLE connect: {e}");
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

        // user clicked disconnect while already disconnected, or timeout fired
        Some(BgCmd::Disconnect) | None | Some(BgCmd::FlashFirmware(_)) => None,

        // can happen if UI fires a write right as we disconnect
        Some(BgCmd::WriteRegs(_) | BgCmd::ReadParams) => {
            tracing::warn!("got write/read cmd while disconnected, dropping");
            None
        }

        Some(BgCmd::RetryVpn) => { vpn_notify.notify_one(); None }
    }
}

/// reads all register batches via FC04, merges values, persists to `SQLite`.
/// returns true if any batch had an error (CRC failure, timeout, etc).
async fn do_poll_cycle(
    ctx: &mut tokio_modbus::client::Context,
    batches: &[RegBatch],
    st: &SharedState,
    db: &crate::storage::HistoryDb,
) -> bool {
    let t0 = std::time::Instant::now();
    let mut merged: HashMap<String, crate::device_descriptor::RegisterValue> = HashMap::new();
    let mut any_err = false;

    for b in batches {
        // fC04 read_input_registers, this is telemetry data (temps, pressures, etc)
        let resp = ctx.read_input_registers(b.base_addr, b.count).await;
        match resp {
            Ok(Ok(raw)) => {
                for entry in &b.entries {
                    let i = usize::from(entry.offset);
                    if i >= raw.len() {
                        // descriptor mentions regs the firmware doesn't expose yet -
                        // happens when cloud descriptor is updated before device FW rollout
                        tracing::debug!(reg = %entry.register.id, offset = i, got = raw.len(), "register past response boundary");
                        continue;
                    }
                    merged.insert(entry.register.id.clone(), entry.register.decode(raw[i]));
                }
            }
            Ok(Err(exc)) => {
                // modbus exception, device explicitly said no
                any_err = true;
                tracing::warn!(base = b.base_addr, "FC04 exception: {exc}");
            }
            Err(e) => {
                any_err = true;
                // cRC errors are routine on RS-485 runs longer than ~15m,
                // especially in panels with VFDs switching nearby. Don't
                // escalate to error level or the log fills up fast.
                tracing::warn!(base = b.base_addr, "batch read failed: {e}");
            }
        }
    }

    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;

    // >500ms usually means the adapter is retransmitting internally (CRC retry).
    // cross-reference with CRC error count to distinguish cable vs adapter issues.
    if elapsed_ms > 500.0 {
        tracing::debug!(ms = elapsed_ms, n = batches.len(), "slow poll cycle");
    }

    if !merged.is_empty() { db.insert_poll(&merged); }

    let mut s = st.write().unwrap();
    s.poll_error_count = if any_err { s.poll_error_count + 1 } else { 0 };
    let n = merged.len();
    s.register_values = merged;
    s.last_poll_ms = Some(
        u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0),
    );
    // tack on "(partial)" so field techs know data might be incomplete
    let sfx = if any_err { " (partial)" } else { "" };
    s.push_log(LogLevel::Info, format!("Polled {n} regs in {elapsed_ms:.0}ms{sfx}"));
    drop(s);

    any_err
}

async fn handle_while_connected(
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
                if prepared.is_empty() {
                    tracing::debug!("nothing to write after encoding");
                    continue;
                }
                crate::modbus::writer::execute_writes(mb, &prepared, st).await;
                // read-back: some the client inverters silently clamp values to
                // their internal min/max and we need the UI to show what actually
                // stuck, not what the user typed
                crate::modbus::writer::read_parameters(mb, params, st).await;
            }
            BgCmd::ReadParams => {
                crate::modbus::writer::read_parameters(ctx.as_mut().expect("connected"), params, st).await;
            }
            BgCmd::ScanPorts => enumerate_serial(st),
            BgCmd::ScanBle => { ble::scanner::scan_ble_devices(st).await; }
            BgCmd::ConnectSerial(_) | BgCmd::ConnectBle(_) | BgCmd::ConnectEmulated => {
                // uI shouldn't send this but it does sometimes during rapid reconnect
                tracing::warn!("connect cmd while already connected - ignoring");
            }
            BgCmd::RetryVpn => { vpn_notify.notify_one(); }
            BgCmd::FlashFirmware(blob) => {
                let mb = ctx.as_mut().expect("connected");
                tracing::info!(bytes = blob.len(), "OTA flash starting");
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
                        // rev A boards have no bootloader partition, they NAK the OTA
                        // control register. Not really an error, just old hardware.
                        tracing::warn!("OTA not supported (probably rev A hw)");
                        st.write().unwrap().push_log(LogLevel::Error, "OTA not available on this hardware revision");
                    }
                    crate::firmware::ota::OtaResult::Failed(reason) => {
                        tracing::error!(%reason, "OTA failed");
                        st.write().unwrap().push_log(LogLevel::Error, format!("OTA failed: {reason}"));
                    }
                }
            }
        }
    }
    ctx
}
