use crate::device_descriptor::{Register, RegisterValue};
use crate::modbus::types::build_batches;
use crate::state::{LogLevel, SharedState};

use std::collections::HashMap;
use tokio_modbus::prelude::{Reader, Writer};

/// commands sent from the UI thread to the background Modbus poller.
/// using an enum instead of separate channels because we tried that first
/// and the ordering guarantees were a nightmare, write followed by read-back
/// needs to happen in order, and with separate channels they'd race.
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

// FIXME(rc): BackgroundCommand should probably carry a oneshot::Sender
// so the UI can await confirmation instead of polling state
// type BgCmdWithReply = (BackgroundCommand, tokio::sync::oneshot::Sender<bool>);

/// builds (address, `raw_value`, id) tuples for writing.
/// silently drops unknown IDs, the UI can send stale register IDs if the
/// user switches devices while a write request is in flight.
pub fn prepare_writes(
    writes: &[(String, f64)],
    params: &[Register],
) -> Vec<(u16, u16, String)> {
    let mut out = Vec::with_capacity(writes.len());
    for (id, val) in writes {
        // linear scan is fine, param lists are 10-30 entries max on the client
        let Some(reg) = params.iter().find(|r| r.id == *id) else { continue };
        if let Some(addr) = reg.address {
            out.push((addr, reg.encode(*val), id.clone()));
        }
    }
    out
}

/// writes registers one at a time via FC06 (write single register).
/// returns how many succeeded. We don't batch writes because the client
/// inverters need a small delay between register writes (empirically
/// ~50ms, but tokio-modbus already adds frame gaps so it works out).
pub async fn execute_writes(
    ctx: &mut tokio_modbus::client::Context,
    writes: &[(u16, u16, String)],
    st: &SharedState,
) -> usize {
    let mut ok_count = 0usize;

    for (addr, raw, id) in writes {
        match ctx.write_single_register(*addr, *raw).await {
            Ok(Ok(())) => {
                ok_count += 1;
                if let Ok(mut s) = st.write() {
                    s.push_log(LogLevel::Info, format!("Wrote {id} [0x{addr:04X}] = {raw}"));
                }
            }
            Ok(Err(exc)) => {
                // modbus exception = device explicitly rejected the write.
                // usually a read-only register or value out of range.
                // we used to log this at Info and nobody noticed for weeks
                // until the Joinville tech filed ticket #47 about "parameters
                // not saving". Now it's Error so it shows up red in the UI.
                tracing::warn!("write {id} got Modbus exception: {exc}");
                if let Ok(mut s) = st.write() {
                    s.push_log(LogLevel::Error, format!("Device rejected write to {id}: {exc}"));
                }
            }
            Err(e) => {
                // transport-level: cable yank, timeout, CRC failure
                tracing::warn!("write {id} @ 0x{addr:04X}: {e}");
                st.write().unwrap().push_log(LogLevel::Info, format!("Write failed for {id}: {e}"));
            }
        }
    }
    ok_count
}

/// read back all writable parameters via FC03 (holding registers).
/// partial failures are fine, we keep whatever we got. This happens
/// surprisingly often after FW updates; the param table sometimes needs
/// a power cycle before it responds to FC03 again.
pub async fn read_parameters(
    ctx: &mut tokio_modbus::client::Context,
    params: &[Register],
    st: &SharedState,
) {
    let batches = build_batches(params);
    let mut vals: HashMap<String, RegisterValue> = HashMap::new();
    let mut n_failed = 0u32;

    for batch in &batches {
        // fC03 read_holding_registers
        let resp = ctx.read_holding_registers(batch.base_addr, batch.count).await;
        match resp {
            Ok(Ok(raw)) => {
                for br in &batch.entries {
                    let idx = usize::from(br.offset);
                    if idx < raw.len() {
                        vals.insert(br.register.id.clone(), br.register.decode(raw[idx]));
                    }
                }
            }
            Ok(Err(exc)) => {
                tracing::debug!(addr = batch.base_addr, "param batch FC03 exception: {exc}");
                n_failed += 1;
            }
            Err(e) => {
                tracing::debug!(addr = batch.base_addr, "param batch read failed: {e}");
                n_failed += 1;
            }
        }
    }

    let mut s = st.write().unwrap();
    let count = vals.len();
    s.parameter_values = vals;
    if n_failed > 0 {
        s.push_log(LogLevel::Info, format!("Read {count} params ({n_failed} batch(es) failed)"));
    } else {
        s.push_log(LogLevel::Info, format!("Read {count} parameter values"));
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

    // multiplier encoding: 15.2 * 10.0 = 152 raw, 2.0 * 10.0 = 20 raw
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

    // stale ID from a previous device session, should just be dropped
    #[test]
    fn stale_id_dropped() {
        let params = vec![mkparam("KNOWN", Some(10), None)];
        let w = prepare_writes(&[("GHOST".into(), 5.0)], &params);
        assert!(w.is_empty());
    }

    // computed params like COP have no modbus address
    #[test]
    fn no_address_skipped() {
        let params = vec![mkparam("COP_CALC", None, None)];
        assert!(prepare_writes(&[("COP_CALC".into(), 5.0)], &params).is_empty());
    }
}
