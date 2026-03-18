use crate::device_descriptor::{Register, RegisterValue};
use crate::modbus::types::build_batches;
use crate::state::{LogLevel, SharedState};

use std::collections::HashMap;
use tokio_modbus::prelude::{Reader, Writer};

#[derive(Debug, Clone)]
pub enum BackgroundCommand {
    WriteRegs(Vec<(String, f64)>),
    ReadParams,
    ConnectSerial(String),
    ConnectBle(String),
    ConnectEmulated,
    Disconnect,
    ScanPorts,
    ScanBle,
    RetryVpn,
    FlashFirmware(Vec<u8>),
}
pub type BgCmd = BackgroundCommand;


pub fn prepare_writes(
    writes: &[(String, f64)],
    params: &[Register],
) -> Vec<(u16, u16, String)> {
    let mut res = Vec::with_capacity(writes.len());
    for (id, v) in writes {
        let Some(r) = params.iter().find(|x| x.id == *id) else { continue };
        if let Some(a) = r.address {
            res.push((a, r.encode(*v), id.clone()));
        }
    }
    res
}

// FC06 write 
pub async fn execute_writes(
    ctx: &mut tokio_modbus::client::Context,
    writes: &[(u16, u16, String)],
    st: &SharedState,
) -> usize {
    let mut n = 0usize;

    for (addr, raw, id) in writes {
        match ctx.write_single_register(*addr, *raw).await {
            Ok(Ok(())) => {
                n += 1;
                if let Ok(mut s) = st.write() {
                    s.push_log(LogLevel::Info, format!("Wrote {id} [0x{addr:04X}] = {raw}"));
                }
            }
            Ok(Err(exc)) => {
                if let Ok(mut s) = st.write() {
                    s.push_log(LogLevel::Error, format!("Device rejected write to {id}: {exc}"));
                }
            }
            Err(e) => {
                st.write().unwrap().push_log(LogLevel::Info, format!("Write failed for {id}: {e}"));
            }
        }
    }
    n
}

pub async fn read_parameters(
    ctx: &mut tokio_modbus::client::Context,
    params: &[Register],
    st: &SharedState,
) {
    let tmp = build_batches(params);
    let mut data: HashMap<String, RegisterValue> = HashMap::new();
    let mut errs = 0u32;

    for b in &tmp {
        // fC03 read_holding_registers
        match ctx.read_holding_registers(b.base_addr, b.count).await {
            Ok(Ok(raw)) => {
                for x in &b.entries {
                    let i = usize::from(x.offset);
                    if i < raw.len() {
                        data.insert(x.register.id.clone(), x.register.decode(raw[i]));
                    }
                }
            }
            Ok(Err(_exc)) => { errs += 1; }
            Err(_e) => { errs += 1; }
        }
    }

    let mut s = st.write().unwrap();
    let n = data.len();
    s.parameter_values = data;
    if errs > 0 {
        s.push_log(LogLevel::Info, format!("Read {n} params ({errs} batch(es) failed)"));
    } else {
        s.push_log(LogLevel::Info, format!("Read {n} parameter values"));
    }
    drop(s);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_descriptor::Register;

    fn mkparam(id: &str, addr: Option<u16>, mult: Option<f64>) -> Register {
        Register {
            id: id.to_string(),
            register_type: Some("integer".to_string()),
            name: None,
            acronym: None,
            description: None,
            address: addr,
            min_value: None,
            max_value: None,
            default_value: None,
            multiplier: mult,
            unit: None,
            is_read_only: None,
            is_write_only: None,
            is_visible: None,
            read_access_level: None,
            write_access_level: None,
            hw_sw_set_mask: None,
            is_delta: None,
            in_chart: None,
            fields: vec![],
        }
    }

    #[test]
    fn encode_with_multiplier() {
        let params = vec![
            mkparam("SETPOINT", Some(16), Some(10.0)),
            mkparam("DIFF", Some(18), Some(10.0)),
        ];
        let w = prepare_writes(
            &[("SETPOINT".into(), 15.2), ("DIFF".into(), 2.0)],
            &params,
        );
        assert_eq!(w.len(), 2);
        assert_eq!(w[0], (16, 152, "SETPOINT".to_string()));
        assert_eq!(w[1], (18, 20, "DIFF".to_string()));
    }

    #[test]
    fn stale_id_dropped() {
        let params = vec![mkparam("KNOWN", Some(10), None)];
        let w = prepare_writes(&[("GHOST".into(), 5.0)], &params);
        assert!(w.is_empty());
    }

    #[test]
    fn no_address_skipped() {
        let params = vec![mkparam("COP_CALC", None, None)];
        assert!(prepare_writes(&[("COP_CALC".into(), 5.0)], &params).is_empty());
    }
}
