// register map for OTA firmware update. Addresses were reverse-engineered from
// the client's Windows flasher, the official docs only mention 60000-60009 for
// "device identity". Everything above 60100 is bootloader-specific and NOT in
// any datasheet I could find.

/// firmware version lives at 60001 (NOT 60000, that's the device model ID).
/// format: major in high byte, minor in low byte. e.g. 0x0201 = v2.01
pub const REG_FIRMWARE_VERSION: u16 = 60001;

//, OTA control/status register. Writing commands here, reading status back.
// the bootloader re-uses the same register for both which is... a choice.
pub const REG_OTA_CONTROL: u16 = 60100;

// firmware size split across two 16-bit regs (big-endian). I spent two hours
// debugging because I initially assumed little-endian like the rest of Modbus.
// nope. the client's bootloader is big-endian for the size field only. Cool.
pub const REG_FW_SIZE_HIGH: u16 = 60101;
pub const REG_FW_SIZE_LOW: u16 = 60102;

// cRC32 of the entire firmware blob, also big-endian split.
pub const REG_CRC_HIGH: u16 = 60103;
pub const REG_CRC_LOW: u16 = 60104;

// data window: 128 consecutive holding registers = 256 bytes per chunk.
// the bootloader expects you to fill the entire window before signaling
// "chunk ready". Partial writes cause silent corruption, no error, just
// a bricked board. Ask me how I know.
pub const REG_DATA_WINDOW_START: u16 = 60105;
pub const REG_DATA_WINDOW_END: u16 = 60232;

/// 256 bytes per chunk. This matches what the Windows flasher sends.
/// tried 512 once, bootloader just ignores the extra registers.
pub const CHUNK_SIZE: usize = 256;
// verified empirically: the bootloader's data window is exactly 128
// registers (256 bytes). Writing beyond 60232 is silently ignored.
// Leaving this assertion here in case a future firmware revision
// extends the window.
#[allow(dead_code)]
const _EXPECTED_WINDOW_REGS: u16 = 128;
// dbg!(DATA_WINDOW_REGISTERS == _EXPECTED_WINDOW_REGS);

#[cfg(test)]
const DATA_WINDOW_REGISTERS: u16 = REG_DATA_WINDOW_END - REG_DATA_WINDOW_START + 1;

// oTA commands written to REG_OTA_CONTROL
pub const OTA_CMD_START: u16 = 1;
pub const OTA_CMD_COMMIT: u16 = 2;
pub const OTA_CMD_ABORT: u16 = 3;
// chunk acknowledgment is 0x10 + chunk_index, handled inline in ota.rs

// oTA status values read back from REG_OTA_CONTROL
pub const OTA_STATUS_IDLE: u16 = 0;
pub const OTA_STATUS_RECEIVING: u16 = 1;
pub const OTA_STATUS_VALIDATING: u16 = 2;
pub const OTA_STATUS_SUCCESS: u16 = 3;
pub const OTA_STATUS_ERROR: u16 = 4;
// FIXME: there might be a status 5 ("erasing flash") that I saw once in a
// wireshark capture but couldn't reproduce. Ignoring for now.

/// uI-facing OTA status. This gets shown in the dashboard panel so the user
/// knows what's going on during a flash that can take 2+ minutes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OtaStatus {
    Idle,
    #[allow(dead_code, reason = "variant reserved for future OTA probe UI state")]
    Probing,
    #[allow(dead_code, reason = "variant reserved for future OTA download UI state")]
    Downloading,
    Flashing { progress_pct: u8 },
    Validating,
    Success { new_version: u16 },
    Failed { reason: String },
}

impl std::fmt::Display for OtaStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Probing => write!(f, "Probing"),
            Self::Downloading => write!(f, "Downloading"),
            Self::Flashing { progress_pct: pct } => write!(f, "Flashing ({pct}%)"),
            Self::Validating => write!(f, "Validating"),
            Self::Success { new_version: ver } => {
                // same encoding as REG_FIRMWARE_VERSION
                let maj = ver >> 8;
                let min = ver & 0xFF;
                write!(f, "Success (v{maj}.{min:02})")
            }
            Self::Failed { reason } => write!(f, "Failed: {reason}"),
        }
    }
}

/// cRC32 (ISO 3309 / ITU-T V.42). The bootloader checks this before committing
/// the flash. Polynomial 0xEDB88320 (reflected). I verified the output against
/// the `crc32` command on Linux and against binutils for a known .bin file.
pub fn crc32(data: &[u8]) -> u32 {
    // standard reflected CRC32, nothing fancy
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    // sanity: empty input should give CRC32 of nothing (0x00000000)
    #[test]
    fn crc32_empty() {
        assert_eq!(crc32(&[]), 0x0000_0000);
    }

    // checked against: echo -n "hello" | crc32 (from libarchive-tools)
    #[test]
    fn crc32_hello() {
        assert_eq!(crc32(b"hello"), 0x3610_A686, "mismatch vs linux crc32 tool");
    }

    #[test]
    fn crc32_all_zeros() {
        assert_eq!(crc32(&[0u8; 4]), 0x2144_DF1C);
    }

    #[test]
    fn crc32_0xff_byte() {
        // edge case: 0xFF is the flash erased state on NXP parts
        assert_eq!(crc32(&[0xFF_u8]), 0xFF00_0000);
    }

    // make sure version formatting round-trips correctly
    #[test]
    fn display_success_version_encoding() {
        let s = OtaStatus::Success { new_version: 0x0201 };
        assert_eq!(format!("{s}"), "Success (v2.01)");

        // v1.00
        let s2 = OtaStatus::Success { new_version: 0x0100 };
        assert_eq!(format!("{s2}"), "Success (v1.00)");
    }

    #[test]
    fn display_flashing_pct() {
        assert_eq!(format!("{}", OtaStatus::Flashing { progress_pct: 0 }), "Flashing (0%)");
        assert_eq!(format!("{}", OtaStatus::Flashing { progress_pct: 42 }), "Flashing (42%)");
        assert_eq!(format!("{}", OtaStatus::Flashing { progress_pct: 100 }), "Flashing (100%)");
    }

    #[test]
    fn display_simple_variants() {
        // just making sure these don't panic, nothing fancy
        assert_eq!(OtaStatus::Idle.to_string(), "Idle");
        assert_eq!(OtaStatus::Probing.to_string(), "Probing");
        assert_eq!(OtaStatus::Downloading.to_string(), "Downloading");
        assert_eq!(OtaStatus::Validating.to_string(), "Validating");
    }

    #[test]
    fn display_failed() {
        let s = OtaStatus::Failed { reason: "CRC mismatch".into() };
        assert_eq!(format!("{s}"), "Failed: CRC mismatch");
    }

    // all registers must be in the 60000+ range (the client "extended" block).
    // this caught a copy-paste bug once where I accidentally wrote 6010 instead
    // of 60100.
    #[test]
    fn registers_in_extended_block() {
        assert!(REG_FIRMWARE_VERSION >= 60000);
        assert!(REG_OTA_CONTROL >= 60000);
        assert!(REG_FW_SIZE_HIGH >= 60000);
        assert!(REG_FW_SIZE_LOW >= 60000);
        assert!(REG_CRC_HIGH >= 60000);
        assert!(REG_CRC_LOW >= 60000);
        assert!(REG_DATA_WINDOW_START >= 60000);
        assert!(REG_DATA_WINDOW_END >= 60000);
    }

    // 128 regs * 2 bytes each = 256 byte chunk
    #[test]
    fn data_window_matches_chunk_size() {
        assert_eq!(DATA_WINDOW_REGISTERS, 128);
        assert_eq!(DATA_WINDOW_REGISTERS as usize * 2, CHUNK_SIZE);
    }

    // paranoia: the constants must not collide
    #[test]
    fn ota_commands_distinct() {
        let cmds = [OTA_CMD_START, OTA_CMD_COMMIT, OTA_CMD_ABORT];
        for i in 0..cmds.len() {
            for j in (i + 1)..cmds.len() {
                assert_ne!(cmds[i], cmds[j]);
            }
        }
    }

    #[test]
    fn ota_status_codes_distinct() {
        let vals = [OTA_STATUS_IDLE, OTA_STATUS_RECEIVING, OTA_STATUS_VALIDATING,
                    OTA_STATUS_SUCCESS, OTA_STATUS_ERROR];
        for i in 0..vals.len() {
            for j in (i+1)..vals.len() {
                assert_ne!(vals[i], vals[j]);
            }
        }
    }

    // register layout: control < size < crc < data window
    // if these ever get reordered the bootloader will silently accept garbage.
    #[test]
    fn register_ordering() {
        assert!(REG_OTA_CONTROL < REG_FW_SIZE_HIGH);
        assert!(REG_FW_SIZE_HIGH < REG_FW_SIZE_LOW);
        assert!(REG_FW_SIZE_LOW < REG_CRC_HIGH);
        assert!(REG_CRC_HIGH < REG_CRC_LOW);
        assert!(REG_CRC_LOW < REG_DATA_WINDOW_START);
        assert!(REG_DATA_WINDOW_START < REG_DATA_WINDOW_END);
    }
}
