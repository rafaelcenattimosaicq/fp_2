
use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

//, xorshift PRNG -------------------------------------------------------
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
        // lCG
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        self.0
    }
}

/// cRC-16/MODBUS (poly 0xA001, init 0xFFFF). Byte-at-a-time
fn crc16(data: &[u8]) -> u16 {
    let mut v: u16 = 0xFFFF;
    for &x in data {
        v ^= u16::from(x);
        for _ in 0..8 {
            if v & 1 != 0 { v = (v >> 1) ^ 0xA001; }
            else { v >>= 1; }
        }
    }
    v
}

// push CRC
fn push_crc(frame: &mut Vec<u8>) {
    let r = crc16(frame);
    #[allow(clippy::cast_possible_truncation, reason = "extracting low byte")]
    frame.push(r as u8);
    #[allow(clippy::cast_possible_truncation, reason = "extracting high byte")]
    frame.push((r >> 8) as u8);
}

fn check_crc(frame: &[u8]) -> bool {
    if frame.len() < 3 { return false; }
    let buf = &frame[..frame.len() - 2];
    let exp = crc16(buf);
    let got = u16::from(frame[frame.len() - 2]) | (u16::from(frame[frame.len() - 1]) << 8);
    exp == got
}

const DEV_ID_ADDR: u16 = 60000;
const DEFAULT_DEV_ID: u16 = 0x0007;

/// fake Modbus RTU slave
pub struct EmulatedTransport {
    inp_regs: HashMap<u16, u16>,
    hold_regs: HashMap<u16, u16>,
    resp_buf: VecDeque<u8>,
    req_buf: Vec<u8>,
    sid: u8, // slave id
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
    pub fn new(slave_id: u8, dev_id: Option<u16>) -> Self {
        let mut hold = HashMap::new();
        hold.insert(DEV_ID_ADDR, dev_id.unwrap_or(DEFAULT_DEV_ID));

        use crate::firmware::types;
        // firmware OTA registers
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

    /// populate input/holding registers
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

    // wobble value so the chart doesn't flatline
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "u16 range fits in f64 just fine"
    )]
    fn jitter(&mut self, base: u16) -> u16 {
        if base == 0 {
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
        if !check_crc(frame) { return; }

        let s = frame[0];
        let f = frame[1];
        if s != self.sid { return; }  // not for us

        let data = match f {
            0x03 => self.fc03_read_holding(frame),
            0x04 => self.fc04_read_input(frame),
            0x06 => self.fc06_write_single(frame),
            _ => {
                let mut r = vec![s, f | 0x80, 0x01];
                push_crc(&mut r);
                r
            }
        };

        self.resp_buf.extend(data);
        if let Some(w) = self.waker.take() { w.wake(); }
    }

    // fC 03
    fn fc03_read_holding(&mut self, frame: &[u8]) -> Vec<u8> {
        let start = u16::from(frame[2]) << 8 | u16::from(frame[3]);
        let cnt = u16::from(frame[4]) << 8 | u16::from(frame[5]);

        let mut r = vec![self.sid, 0x03];
        #[allow(clippy::cast_possible_truncation, reason = "Modbus limits count to 125 regs")]
        r.push((cnt * 2) as u8);

        for i in 0..cnt {
            let a = start + i;
            let base_val = self.hold_regs.get(&a).copied().unwrap_or(0);

            let v = if self.written_addrs.contains(&a) { base_val } else { self.jitter(base_val) };
            r.push((v >> 8) as u8);
            #[allow(clippy::cast_possible_truncation, reason = "low byte extraction")]
            r.push(v as u8);
        }
        push_crc(&mut r);
        r
    }

    // fC 04, Read Input Registers
    // telemetry: suction/discharge temps,
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
    fn fc06_write_single(&mut self, frame: &[u8]) -> Vec<u8> {
        use crate::firmware::types;
        let addr = u16::from(frame[2]) << 8 | u16::from(frame[3]);
        let val = u16::from(frame[4]) << 8 | u16::from(frame[5]);

        if addr == types::REG_OTA_CONTROL {
            self.written_addrs.insert(addr);
            match val {
                types::OTA_CMD_START => {
                    self.ota_active = true;
                    self.ota_fw.clear();
                    self.hold_regs.insert(addr, types::OTA_STATUS_RECEIVING);
                }
                types::OTA_CMD_COMMIT => {
                    self.hold_regs.insert(addr, types::OTA_STATUS_VALIDATING);

                    let actual_crc = types::crc32(&self.ota_fw);

                    #[allow(clippy::cast_possible_truncation, reason = "firmware size is always well under 4GB")]
                    if actual_crc == self.ota_crc && self.ota_fw.len() as u32 == self.ota_sz {
                        let cur = self.hold_regs.get(&types::REG_FIRMWARE_VERSION).copied().unwrap_or(0x0100);
                        let nv = cur + 1;
                        self.hold_regs.insert(types::REG_FIRMWARE_VERSION, nv);
                        self.written_addrs.insert(types::REG_FIRMWARE_VERSION);
                        self.hold_regs.insert(addr, types::OTA_STATUS_SUCCESS);
                    } else {
                        self.hold_regs.insert(addr, types::OTA_STATUS_ERROR);
                    }
                    self.ota_active = false;
                }
                types::OTA_CMD_ABORT => {
                    self.ota_active = false;
                    self.ota_fw.clear();
                    self.hold_regs.insert(addr, types::OTA_STATUS_IDLE);
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
            self.ota_fw.extend_from_slice(&val.to_be_bytes());
            self.hold_regs.insert(addr, val);
            self.written_addrs.insert(addr);
        } else {
            self.hold_regs.insert(addr, val);
            self.written_addrs.insert(addr);
        }

        let mut resp = vec![self.sid, 0x06, frame[2], frame[3], frame[4], frame[5]];
        push_crc(&mut resp);
        resp
    }

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

// asyncRead
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
        // drain 
        let n = this.resp_buf.len().min(buf.remaining());
        for _ in 0..n {
            if let Some(b) = this.resp_buf.pop_front() {
                buf.put_slice(&[b]);
            }
        }
        Poll::Ready(Ok(()))
    }
}

// asyncWrite, the "serial port" write side
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

    // pull the first reg
    fn drain_resp_reg(t: &mut EmulatedTransport) -> u16 {
        let resp: Vec<u8> = t.resp_buf.drain(..).collect();
        assert!(resp.len() >= 7, "response too short: {} bytes", resp.len());
        assert!(check_crc(&resp));
        u16::from(resp[3]) << 8 | u16::from(resp[4])
    }

    #[test]
    fn crc16_known_vector() {
        // verified 
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
        // tEMP at input reg 100
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

        assert_eq!(t.hold_regs.get(&5), Some(&0x00FF));

        // echo identical
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
