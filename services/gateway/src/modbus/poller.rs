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
