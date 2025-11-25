// emulated Modbus RTU transport, pretends to be a real device connected over RS-485.
// used for development on macOS where there's no FTDI adapter plugged in.
// register 60000 holds the device ID: 0x0007 = AMBIENT-SENSOR, 0x0008 = VEMB compressor.
// jitter on register reads keeps the chart looking alive during demos, real the client
// compressor telemetry drifts +-2-3% anyway so nobody notices it's fake.
//
// known quirk: the CRC16 here is the standard Modbus polynomial but we had a bug
// in an earlier version where we forgot to XOR 0xFFFF at init and it took two days
// to figure out why the CH340 clone was "failing", turned out it was us, not the cable.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

//, xorshift PRNG -------------------------------------------------------
// don't want to pull in rand just for jitter on demo values
struct SimpleRng(u64);

impl SimpleRng {
    #[allow(clippy::cast_possible_truncation, reason = "truncation is fine, we only need entropy")]
    fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(12345, |d| d.as_nanos() as u64);
        Self(seed)
    }

    #[allow(clippy::missing_const_for_fn, reason = "&mut self in const fn requires nightly")]
    fn next_u64(&mut self) -> u64 {
        // lCG, good enough for wobbling chart values
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        self.0
    }
}

/// cRC-16/MODBUS (poly 0xA001, init 0xFFFF). Byte-at-a-time, slow but we only
/// run this on 8-byte request frames and short responses so it doesn't matter.
/// spent a whole afternoon once debugging CRC mismatches against a Waveshare 7"
/// rPi display gateway that was using big-endian byte order for the CRC, turns
/// out Modbus wire order is little-endian for the CRC but big-endian for register
/// data. Fun times.
fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            if crc & 1 != 0 { crc = (crc >> 1) ^ 0xA001; }
            else { crc >>= 1; }
        }
    }
    crc
}

// push CRC onto frame in little-endian wire order
fn push_crc(frame: &mut Vec<u8>) {
    let c = crc16(frame);
    #[allow(clippy::cast_possible_truncation, reason = "extracting low byte")]
    frame.push(c as u8);
    #[allow(clippy::cast_possible_truncation, reason = "extracting high byte")]
    frame.push((c >> 8) as u8);
}

fn check_crc(frame: &[u8]) -> bool {
    if frame.len() < 3 { return false; }
    let payload = &frame[..frame.len() - 2];
    let expected = crc16(payload);
    let got = u16::from(frame[frame.len() - 2]) | (u16::from(frame[frame.len() - 1]) << 8);
    expected == got
}

// device ID register, same address the real firmware uses (client spec v3.2)
const DEV_ID_ADDR: u16 = 60_000;
// AMBIENT-SENSOR board from the Joinville pilot
const DEFAULT_DEV_ID: u16 = 0x0007;

/// Fake Modbus RTU slave. Implements `AsyncRead` + `AsyncWrite` so it can be
/// swapped in wherever a real serial port would go. The jitter on reads makes
/// the chart in the dashboard look realistic — field techs at the Joinville
/// pilot actually thought it was a live compressor the first time they saw it.
pub struct EmulatedTransport {
    inp_regs: HashMap<u16, u16>,
    hold_regs: HashMap<u16, u16>,
    resp_buf: VecDeque<u8>,
    req_buf: Vec<u8>,
    sid: u8, // slave id, always 1 in our setup but configurable for multi-drop RS-485
    waker: Option<Waker>,
    rng: SimpleRng,
    written_addrs: HashSet<u16>,  // addresses that were FC06'd, skip jitter on readback
    // oTA state machine
    ota_active: bool,
    ota_fw: Vec<u8>,
    ota_sz: u32,
    ota_crc: u32,
}

impl EmulatedTransport {
    /// create a new emulated device. `dev_id` overrides the value at register 60000
    /// (pass None for the default AMBIENT-SENSOR 0x0007).
    pub fn new(slave_id: u8, dev_id: Option<u16>) -> Self {
        let mut hold = HashMap::new();
        hold.insert(DEV_ID_ADDR, dev_id.unwrap_or(DEFAULT_DEV_ID));

        use crate::firmware::types;
        // firmware OTA registers, seed with sane defaults so reads before
        // any OTA flow don't return garbage
        hold.insert(types::REG_FIRMWARE_VERSION, 0x0100);
        hold.insert(types::REG_OTA_CONTROL, types::OTA_STATUS_IDLE);

        let mut wr = HashSet::new();
        wr.insert(types::REG_FIRMWARE_VERSION);
        wr.insert(types::REG_OTA_CONTROL);

        Self {
            inp_regs: HashMap::new(),
            hold_regs: hold,
            resp_buf: VecDeque::new(),
            req_buf: Vec::new(),
            sid: slave_id,
            waker: None,
            rng: SimpleRng::new(),
            written_addrs: wr,
            ota_active: false,
            ota_fw: Vec::new(),
            ota_sz: 0,
            ota_crc: 0,
        }
    }

    /// populate input/holding registers from a device descriptor so the emulated
    /// device returns plausible values. Called once after we fetch the descriptor
    /// from the cloud API (or local fallback).
    #[cfg(test)]
    pub fn seed_registers(
        &mut self,
        status: &[crate::device_descriptor::Register],
        params: &[crate::device_descriptor::Register],
    ) {
        for r in status {
            if let Some(a) = r.address {
                let raw = r.encode(r.default_value.unwrap_or(0.0));
                self.inp_regs.insert(a, raw);
            }
        }
        // holding regs are read/write, setpoints, thresholds, etc
        for r in params {
            if let Some(a) = r.address {
                self.hold_regs.insert(a, r.encode(r.default_value.unwrap_or(0.0)));
            }
        }
    }

    // wobble value so the chart doesn't flatline. real compressor readings drift
    // anyway from thermal noise in the ADC on the the client board.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "u16 range fits in f64 just fine"
    )]
    fn jitter(&mut self, base: u16) -> u16 {
        if base == 0 {
            // zero-centered: pick a small random offset
            let raw = self.rng.next_u64();
            let norm = (raw as f64 / u64::MAX as f64).mul_add(2.0, -1.0);
            let off = (norm * 3.0).round() as i32;
            off.unsigned_abs() as u16
        } else {
            let amp = f64::from(base) * 0.05;
            let raw = self.rng.next_u64();
            let norm = (raw as f64 / u64::MAX as f64).mul_add(2.0, -1.0);
            let j = f64::from(base) + norm * amp;
            j.round().clamp(0.0, f64::from(u16::MAX)) as u16
        }
    }

    /// process a complete Modbus RTU frame and push the response into `resp_buf`.
    fn process_frame(&mut self, frame: &[u8]) {
        // minimum frame: slave(1) + fc(1) + data(2+) + crc(2) = 6
        if frame.len() < 6 { return; }
        if !check_crc(frame) { return; } // cRC errors happen a lot on long RS-485 runs (>15m)

        let slave = frame[0];
        let fc = frame[1];
        if slave != self.sid { return; }  // not for us

        // dispatch, only support the three FCs the gateway actually uses
        let resp = match fc {
            0x03 => self.fc03_read_holding(frame),
            0x04 => self.fc04_read_input(frame),
            0x06 => self.fc06_write_single(frame),
            _ => {
                // exception response: illegal function
                let mut r = vec![slave, fc | 0x80, 0x01];
                push_crc(&mut r);
                r
            }
        };

        self.resp_buf.extend(resp);
        if let Some(w) = self.waker.take() { w.wake(); }
    }

    // fC 03, Read Holding Registers
    // used for device ID, firmware version, setpoints, OTA status
    fn fc03_read_holding(&mut self, frame: &[u8]) -> Vec<u8> {
        let start = u16::from(frame[2]) << 8 | u16::from(frame[3]);
        let cnt = u16::from(frame[4]) << 8 | u16::from(frame[5]);

        let mut r = vec![self.sid, 0x03];
        #[allow(clippy::cast_possible_truncation, reason = "Modbus limits count to 125 regs")]
        r.push((cnt * 2) as u8);

        for i in 0..cnt {
            let a = start + i;
            let base_val = self.hold_regs.get(&a).copied().unwrap_or(0);
            // don't jitter values the master wrote, if we wrote a setpoint of 42
            // we should read back 42, not 43
            let v = if self.written_addrs.contains(&a) { base_val } else { self.jitter(base_val) };
            r.push((v >> 8) as u8);
            #[allow(clippy::cast_possible_truncation, reason = "low byte extraction")]
            r.push(v as u8);
        }
        push_crc(&mut r);
        r
    }

    // fC 04, Read Input Registers
    // telemetry: suction/discharge temps, compressor RPM, power, etc
    fn fc04_read_input(&mut self, frame: &[u8]) -> Vec<u8> {
        let start = u16::from(frame[2]) << 8 | u16::from(frame[3]);
        let cnt = u16::from(frame[4]) << 8 | u16::from(frame[5]);

        let mut r = vec![self.sid, 0x04];
        #[allow(clippy::cast_possible_truncation, reason = "Modbus count fits in u8")]
        r.push((cnt * 2) as u8);

        for i in 0..cnt {
            let a = start + i;
            let base_val = self.inp_regs.get(&a).copied().unwrap_or(0);
            let v = self.jitter(base_val);  // always jitter input regs
            r.push((v >> 8) as u8);
            #[allow(clippy::cast_possible_truncation, reason = "low byte")]
            r.push(v as u8);
        }
        push_crc(&mut r);
        r
    }

    // fC 06, Write Single Register
    // this one got complicated once OTA was added, the control register at 60100
    // drives a whole state machine (START -> data chunks -> COMMIT/ABORT)
    fn fc06_write_single(&mut self, frame: &[u8]) -> Vec<u8> {
        use crate::firmware::types;
        let addr = u16::from(frame[2]) << 8 | u16::from(frame[3]);
        let val = u16::from(frame[4]) << 8 | u16::from(frame[5]);

        // oTA control register has special handling
        if addr == types::REG_OTA_CONTROL {
            self.written_addrs.insert(addr);
            match val {
                types::OTA_CMD_START => {
                    self.ota_active = true;
                    self.ota_fw.clear();
                    self.hold_regs.insert(addr, types::OTA_STATUS_RECEIVING);
                    tracing::info!("Emulated OTA: START received");
                }
                types::OTA_CMD_COMMIT => {
                    self.hold_regs.insert(addr, types::OTA_STATUS_VALIDATING);
                    tracing::info!(
                        size = self.ota_fw.len(),
                        expected = self.ota_sz,
                        "Emulated OTA: COMMIT received, validating"
                    );

                    let actual_crc = types::crc32(&self.ota_fw);
                    // FIXME: should we add a small delay here to simulate flash erase time?
                    // real devices take ~200ms for the erase cycle
                    #[allow(clippy::cast_possible_truncation, reason = "firmware size is always well under 4GB")]
                    if actual_crc == self.ota_crc && self.ota_fw.len() as u32 == self.ota_sz {
                        let cur = self.hold_regs.get(&types::REG_FIRMWARE_VERSION).copied().unwrap_or(0x0100);
                        let nv = cur + 1;
                        self.hold_regs.insert(types::REG_FIRMWARE_VERSION, nv);
                        self.written_addrs.insert(types::REG_FIRMWARE_VERSION);
                        self.hold_regs.insert(addr, types::OTA_STATUS_SUCCESS);
                        tracing::info!(new_version = format!("0x{nv:04X}"), "Emulated OTA: flash successful");
                    } else {
                        self.hold_regs.insert(addr, types::OTA_STATUS_ERROR);
                        tracing::warn!(
                            actual_crc, expected_crc = self.ota_crc,
                            actual_size = self.ota_fw.len(), expected_size = self.ota_sz,
                            "Emulated OTA: CRC or size mismatch"
                        );
                    }
                    self.ota_active = false;
                }
                types::OTA_CMD_ABORT => {
                    // user cancelled or timeout, just reset everything
                    self.ota_active = false;
                    self.ota_fw.clear();
                    self.hold_regs.insert(addr, types::OTA_STATUS_IDLE);
                    tracing::info!("Emulated OTA: ABORT received");
                }
                _ if val >= 0x10 && self.ota_active => {
                    // chunk sequence number, just ack it
                    self.hold_regs.insert(addr, types::OTA_STATUS_RECEIVING);
                }
                _ => { self.hold_regs.insert(addr, val); }
            }
        } else if addr == types::REG_FW_SIZE_HIGH {
            self.ota_sz = (u32::from(val) << 16) | (self.ota_sz & 0xFFFF);
            self.hold_regs.insert(addr, val);
            self.written_addrs.insert(addr);
        } else if addr == types::REG_FW_SIZE_LOW {
            self.ota_sz = (self.ota_sz & 0xFFFF_0000) | u32::from(val);
            self.hold_regs.insert(addr, val);  self.written_addrs.insert(addr);
        } else if addr == types::REG_CRC_HIGH {
            self.ota_crc = (u32::from(val) << 16) | (self.ota_crc & 0xFFFF);
            self.hold_regs.insert(addr, val);
            self.written_addrs.insert(addr);
        } else if addr == types::REG_CRC_LOW {
            self.ota_crc = (self.ota_crc & 0xFFFF_0000) | u32::from(val);
            self.hold_regs.insert(addr, val);  self.written_addrs.insert(addr);
        } else if (types::REG_DATA_WINDOW_START..=types::REG_DATA_WINDOW_END).contains(&addr) && self.ota_active {
            // firmware data chunk, accumulate into ota_fw buffer
            self.ota_fw.extend_from_slice(&val.to_be_bytes());
            self.hold_regs.insert(addr, val);
            self.written_addrs.insert(addr);
        } else {
            // generic holding register write, setpoints, thresholds etc
            self.hold_regs.insert(addr, val);
            self.written_addrs.insert(addr);
        }

        // fC06 echo: slave + fc + addr_hi + addr_lo + val_hi + val_lo + crc
        let mut resp = vec![self.sid, 0x06, frame[2], frame[3], frame[4], frame[5]];
        push_crc(&mut resp);
        resp
    }

    // all standard modbus RTU requests are exactly 8 bytes (slave + fc + 4 data + 2 crc)
    // NOTE: FC16 (write multiple) would be variable length but we don't support it
    const fn expected_req_len() -> usize { 8 }
}

impl std::fmt::Debug for EmulatedTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // don't dump all the register maps, it's huge
        f.debug_struct("EmulatedTransport")
            .field("sid", &self.sid)
            .field("inp_regs", &self.inp_regs.len())
            .field("hold_regs", &self.hold_regs.len())
            .field("resp_pending", &self.resp_buf.len())
            .finish_non_exhaustive()
    }
}

// asyncRead, the "serial port" read side. Returns response bytes that were
// queued up by process_frame(). If nothing is ready, park the waker.
impl AsyncRead for EmulatedTransport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.resp_buf.is_empty() {
            this.waker = Some(cx.waker().clone());
            return Poll::Pending;
        }
        // drain as many bytes as the caller's buffer can hold
        let n = this.resp_buf.len().min(buf.remaining());
        for _ in 0..n {
            if let Some(b) = this.resp_buf.pop_front() {
                buf.put_slice(&[b]);
            }
        }
        Poll::Ready(Ok(()))
    }
}

// asyncWrite, the "serial port" write side. Accumulates bytes until we
// have a complete 8-byte request frame, then processes it.
impl AsyncWrite for EmulatedTransport {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let len = buf.len();
        this.req_buf.extend_from_slice(buf);

        // try to consume complete frames
        while this.req_buf.len() >= 2 {
            let exp = Self::expected_req_len();
            if this.req_buf.len() >= exp {
                let frame: Vec<u8> = this.req_buf.drain(..exp).collect();
                this.process_frame(&frame);
            } else {
                break;
            }
        }
        Poll::Ready(Ok(len))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))  // nothing to flush, it's all in memory
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

// -------------------------------------------------------------------------
// tests
// -------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_descriptor::Register;

    fn mk_reg(id: &str, addr: u16, default: Option<f64>, mult: Option<f64>) -> Register {
        Register {
            id: id.to_string(),
            register_type: Some("unsigned integer".to_string()),
            name: None, acronym: None, description: None,
            address: Some(addr),
            min_value: None, max_value: None,
            default_value: default, multiplier: mult,
            unit: None,
            is_read_only: None, is_write_only: None, is_visible: None,
            read_access_level: None, write_access_level: None,
            hw_sw_set_mask: None, is_delta: None, in_chart: None,
            fields: vec![],
        }
    }

    // helper to build a FC06 write-single-register request frame
    fn mk_write(slave: u8, addr: u16, value: u16) -> Vec<u8> {
        let mut f = vec![
            slave, 0x06,
            (addr >> 8) as u8, addr as u8,
            (value >> 8) as u8, value as u8,
        ];
        push_crc(&mut f);
        f
    }

    fn mk_read_hold(slave: u8, addr: u16, count: u16) -> Vec<u8> {
        let mut f = vec![
            slave, 0x03,
            (addr >> 8) as u8, addr as u8,
            (count >> 8) as u8, count as u8,
        ];
        push_crc(&mut f);
        f
    }

    // pull the first register value out of a FC03/FC04 response
    fn drain_resp_reg(t: &mut EmulatedTransport) -> u16 {
        let resp: Vec<u8> = t.resp_buf.drain(..).collect();
        assert!(resp.len() >= 7, "response too short: {} bytes", resp.len());
        assert!(check_crc(&resp));
        u16::from(resp[3]) << 8 | u16::from(resp[4])
    }

    #[test]
    fn crc16_known_vector() {
        // verified against online Modbus CRC calculator
        let data = [0x01, 0x03, 0x00, 0x00, 0x00, 0x01];
        assert_eq!(crc16(&data), 0x0A84, "CRC should match known test vector");
    }

    #[test]
    fn crc_roundtrip_and_corruption() {
        let mut frame = vec![0x01, 0x03, 0x00, 0x00, 0x00, 0x01];
        push_crc(&mut frame);
        assert!(check_crc(&frame));

        // flip a bit, should fail. this catches the kind of errors we see
        // on long RS-485 runs (>15m with cheap unshielded cable from the
        // joinville warehouse)
        let last = frame.len() - 1;
        frame[last] ^= 0xFF;
        assert!(!check_crc(&frame), "corrupted CRC should fail validation");
    }

    #[test]
    fn device_id_register_is_preseeded() {
        let t = EmulatedTransport::new(1, None);
        assert_eq!(t.hold_regs.get(&DEV_ID_ADDR), Some(&DEFAULT_DEV_ID));
    }

    #[test]
    fn seed_registers_populates_both_maps() {
        // tEMP at input reg 100, scaled by 10x. default 25.0 -> raw 250
        let status = vec![mk_reg("TEMP", 100, Some(25.0), Some(10.0))];
        let params = vec![mk_reg("SETPOINT", 200, Some(5.0), Some(10.0))];

        let mut t = EmulatedTransport::new(1, None);
        t.seed_registers(&status, &params);

        assert_eq!(t.inp_regs.get(&100), Some(&250));
        assert_eq!(t.hold_regs.get(&200), Some(&50));
    }

    #[test]
    fn fc03_reads_holding_with_jitter() {
        let mut t = EmulatedTransport::new(1, None);
        let base: u16 = 0x1234;
        t.hold_regs.insert(0, base);

        let mut req = vec![0x01, 0x03, 0x00, 0x00, 0x00, 0x01];
        push_crc(&mut req);
        t.process_frame(&req);

        assert!(!t.resp_buf.is_empty());
        let resp: Vec<u8> = t.resp_buf.drain(..).collect();
        assert_eq!(resp[0], 0x01); // slave
        assert_eq!(resp[1], 0x03); // fc
        assert_eq!(resp[2], 0x02); // byte count
        let actual = u16::from(resp[3]) << 8 | u16::from(resp[4]);
        // jitter is +-5% so allow 6% tolerance to avoid flaky tests
        let tol = f64::from(base) * 0.06;
        assert!((f64::from(actual) - f64::from(base)).abs() <= tol);
        assert!(check_crc(&resp));
    }

    #[test]
    fn fc04_reads_input_with_jitter() {
        let mut t = EmulatedTransport::new(1, None);
        let base: u16 = 0xABCD;
        t.inp_regs.insert(10, base);

        let mut req = vec![0x01, 0x04, 0x00, 0x0A, 0x00, 0x01];
        push_crc(&mut req);
        t.process_frame(&req);

        let resp: Vec<u8> = t.resp_buf.drain(..).collect();
        assert_eq!(resp[0], 0x01);
        assert_eq!(resp[1], 0x04);
        assert_eq!(resp[2], 0x02);
        let actual = u16::from(resp[3]) << 8 | u16::from(resp[4]);
        let tol = f64::from(base) * 0.06;
        assert!(
            (f64::from(actual) - f64::from(base)).abs() <= tol,
            "jittered {actual} should be within +-5% of {base}"
        );
        assert!(check_crc(&resp));
    }

    #[test]
    fn fc06_writes_and_echoes() {
        let mut t = EmulatedTransport::new(1, None);

        let mut req = vec![0x01, 0x06, 0x00, 0x05, 0x00, 0xFF];
        push_crc(&mut req);
        t.process_frame(&req);

        // value should be stored
        assert_eq!(t.hold_regs.get(&5), Some(&0x00FF));

        // echo should be identical to request (minus our CRC)
        let resp: Vec<u8> = t.resp_buf.drain(..).collect();
        assert_eq!(resp[0], 0x01);
        assert_eq!(resp[1], 0x06);
        assert_eq!(resp[2], 0x00);
        assert_eq!(resp[3], 0x05);
        assert_eq!(resp[4], 0x00);
        assert_eq!(resp[5], 0xFF);
        assert!(check_crc(&resp));
    }

    #[test]
    fn unsupported_fc_returns_exception() {
        let mut t = EmulatedTransport::new(1, None);
        let mut req = vec![0x01, 0x08, 0x00, 0x00, 0x00, 0x00];
        push_crc(&mut req);
        t.process_frame(&req);

        let resp: Vec<u8> = t.resp_buf.drain(..).collect();
        assert_eq!(resp[0], 0x01);
        assert_eq!(resp[1], 0x88, "exception FC = 0x08 | 0x80");
        assert_eq!(resp[2], 0x01);  // illegal function
        assert!(check_crc(&resp));
    }

    #[test]
    fn bad_crc_silently_dropped() {
        // on a real bus this happens when someone plugs in a cheap USB-serial
        // adapter with no ground reference, we just ignore it
        let mut t = EmulatedTransport::new(1, None);
        let req = vec![0x01, 0x03, 0x00, 0x00, 0x00, 0x01, 0xFF, 0xFF];
        t.process_frame(&req);
        assert!(t.resp_buf.is_empty(), "bad CRC should produce no response");
    }

    #[test]
    fn wrong_slave_id_ignored() {
        let mut t = EmulatedTransport::new(1, None);
        let mut req = vec![0x02, 0x03, 0x00, 0x00, 0x00, 0x01];
        push_crc(&mut req);
        t.process_frame(&req);
        assert!(t.resp_buf.is_empty());
    }

    #[test]
    fn unseeded_register_returns_near_zero() {
        // reading a register we never wrote, should get small jitter around 0
        let mut t = EmulatedTransport::new(1, None);
        let mut req = vec![0x01, 0x03, 0x03, 0xE7, 0x00, 0x01]; // addr 999
        push_crc(&mut req);
        t.process_frame(&req);

        let resp: Vec<u8> = t.resp_buf.drain(..).collect();
        let val = u16::from(resp[3]) << 8 | u16::from(resp[4]);
        assert!(val <= 3);
    }

    //, OTA flow tests ---------------------------------------------------
    // these mirror what the real ota.rs does against actual hardware

    #[test]
    fn ota_full_happy_path() {
        use crate::firmware::types;
        let slave = 1;
        let mut t = EmulatedTransport::new(slave, None);

        // 512 bytes of fake firmware
        let fw: Vec<u8> = (0u16..256).flat_map(|i| i.to_be_bytes()).collect();
        assert_eq!(fw.len(), 512, "test firmware should be exactly 512 bytes");
        let fw_crc = types::crc32(&fw);
        let fw_sz = fw.len() as u32;

        // 1) START
        let f = mk_write(slave, types::REG_OTA_CONTROL, types::OTA_CMD_START);
        t.process_frame(&f); t.resp_buf.clear();

        // 2) size + crc metadata
        t.process_frame(&mk_write(slave, types::REG_FW_SIZE_HIGH, (fw_sz >> 16) as u16));
        t.resp_buf.clear();
        t.process_frame(&mk_write(slave, types::REG_FW_SIZE_LOW, (fw_sz & 0xFFFF) as u16));
        t.resp_buf.clear();
        t.process_frame(&mk_write(slave, types::REG_CRC_HIGH, (fw_crc >> 16) as u16));
        t.resp_buf.clear();
        t.process_frame(&mk_write(slave, types::REG_CRC_LOW, (fw_crc & 0xFFFF) as u16));
        t.resp_buf.clear();

        // 3) data chunks
        for (ci, chunk) in fw.chunks(types::CHUNK_SIZE).enumerate() {
            for (j, pair) in chunk.chunks(2).enumerate() {
                let v = u16::from_be_bytes([pair[0], pair[1]]);
                t.process_frame(&mk_write(slave, types::REG_DATA_WINDOW_START + j as u16, v));
                t.resp_buf.clear();
            }
            // chunk sequence ack
            t.process_frame(&mk_write(slave, types::REG_OTA_CONTROL, 0x10 + ci as u16));
            t.resp_buf.clear();
        }

        // 4) COMMIT
        t.process_frame(&mk_write(slave, types::REG_OTA_CONTROL, types::OTA_CMD_COMMIT));
        t.resp_buf.clear();

        // verify status = SUCCESS
        t.process_frame(&mk_read_hold(slave, types::REG_OTA_CONTROL, 1));
        let st = drain_resp_reg(&mut t);
        assert_eq!(st, types::OTA_STATUS_SUCCESS);

        // verify firmware version bumped
        t.process_frame(&mk_read_hold(slave, types::REG_FIRMWARE_VERSION, 1));
        let ver = drain_resp_reg(&mut t);
        assert_eq!(ver, 0x0101, "version should bump from 0x0100 to 0x0101 after OTA");
    }

    #[test]
    fn ota_bad_crc_reports_error() {
        use crate::firmware::types;
        let slave = 1;
        let mut t = EmulatedTransport::new(slave, None);
        let fw: Vec<u8> = vec![0xAA; 256];
        let fw_sz = fw.len() as u32;
        let bad_crc: u32 = 0xDEAD_BEEF; // intentionally wrong

        t.process_frame(&mk_write(slave, types::REG_OTA_CONTROL, types::OTA_CMD_START));
        t.resp_buf.clear();

        t.process_frame(&mk_write(slave, types::REG_FW_SIZE_HIGH, (fw_sz >> 16) as u16));
        t.resp_buf.clear();
        t.process_frame(&mk_write(slave, types::REG_FW_SIZE_LOW, (fw_sz & 0xFFFF) as u16));
        t.resp_buf.clear();
        t.process_frame(&mk_write(slave, types::REG_CRC_HIGH, (bad_crc >> 16) as u16));
        t.resp_buf.clear();
        t.process_frame(&mk_write(slave, types::REG_CRC_LOW, (bad_crc & 0xFFFF) as u16));
        t.resp_buf.clear();

        for (j, pair) in fw.chunks(2).enumerate() {
            let v = u16::from_be_bytes([pair[0], pair[1]]);
            t.process_frame(&mk_write(slave, types::REG_DATA_WINDOW_START + j as u16, v));
            t.resp_buf.clear();
        }
        t.process_frame(&mk_write(slave, types::REG_OTA_CONTROL, 0x10));
        t.resp_buf.clear();

        t.process_frame(&mk_write(slave, types::REG_OTA_CONTROL, types::OTA_CMD_COMMIT));
        t.resp_buf.clear();

        t.process_frame(&mk_read_hold(slave, types::REG_OTA_CONTROL, 1));
        let st = drain_resp_reg(&mut t);
        assert_eq!(st, types::OTA_STATUS_ERROR, "should report ERROR on CRC mismatch");

        // version should NOT have changed
        t.process_frame(&mk_read_hold(slave, types::REG_FIRMWARE_VERSION, 1));
        let ver = drain_resp_reg(&mut t);
        assert_eq!(ver, 0x0100);
    }

    #[test]
    fn ota_abort_resets_to_idle() {
        use crate::firmware::types;
        let slave = 1;
        let mut t = EmulatedTransport::new(slave, None);

        t.process_frame(&mk_write(slave, types::REG_OTA_CONTROL, types::OTA_CMD_START));
        t.resp_buf.clear();
        t.process_frame(&mk_write(slave, types::REG_OTA_CONTROL, types::OTA_CMD_ABORT));
        t.resp_buf.clear();

        t.process_frame(&mk_read_hold(slave, types::REG_OTA_CONTROL, 1));
        let st = drain_resp_reg(&mut t);
        assert_eq!(st, types::OTA_STATUS_IDLE, "should return to IDLE after ABORT");
    }
}
