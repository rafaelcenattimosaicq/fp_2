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

// device ID register, same address the real firmware uses (the client spec v3.2)
const DEV_ID_ADDR: u16 = 60000;
// 0x0007 = AMBIENT-SENSOR board from the Joinville pilot
const DEFAULT_DEV_ID: u16 = 0x0007;

/// fake Modbus RTU slave. Implements `AsyncRead` + `AsyncWrite` so it can be
/// dropped in wherever a real serial port would go. The jitter on reads makes
/// the chart in the dashboard look realistic, field techs at the Joinville
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

    fn fc04_read_input(&mut self, frame: &[u8]) -> Vec<u8> {
        // TODO: implement FC04
        let mut r = vec![self.sid, 0x04 | 0x80, 0x01];
        push_crc(&mut r);
        r
    }

    fn fc06_write_single(&mut self, frame: &[u8]) -> Vec<u8> {
        // TODO: implement FC06
        frame.to_vec()
    }
}

impl AsyncRead for EmulatedTransport {
    fn poll_read(mut self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        if self.resp_buf.is_empty() {
            if let Some(w) = self.waker.take() { w.wake(); }
            self.waker = Some(_cx.waker().clone());
            return Poll::Pending;
        }
        let n = buf.remaining().min(self.resp_buf.len());
        for b in self.resp_buf.drain(..n) {
            buf.put_slice(&[b]);
        }
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for EmulatedTransport {
    fn poll_write(mut self: Pin<&mut Self>, _cx: &mut Context<'_>, data: &[u8]) -> Poll<io::Result<usize>> {
        self.req_buf.extend_from_slice(data);
        if self.req_buf.len() >= 8 {
            let frame = self.req_buf.clone();
            self.req_buf.clear();
            self.process_frame(&frame);
        }
        Poll::Ready(Ok(data.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl std::fmt::Debug for EmulatedTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmulatedTransport").field("sid", &self.sid).finish()
    }
}
