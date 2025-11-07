use crate::device_descriptor::Register;
use crate::modbus::types::{build_batches, RegBatch};
use crate::modbus::writer::BgCmd;
use crate::state::{ConnectionStatus, LogLevel, SharedState};

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
