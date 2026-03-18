
pub const REG_FIRMWARE_VERSION: u16 = 60001;


pub const REG_OTA_CONTROL: u16 = 60100;


pub const REG_FW_SIZE_HIGH: u16 = 60101;
pub const REG_FW_SIZE_LOW: u16 = 60102;

pub const REG_CRC_HIGH: u16 = 60103;
pub const REG_CRC_LOW: u16 = 60104;


pub const REG_DATA_WINDOW_START: u16 = 60105;
pub const REG_DATA_WINDOW_END: u16 = 60232;


pub const CHUNK_SIZE: usize = 256;

#[cfg(test)]
const DATA_WINDOW_REGISTERS: u16 = REG_DATA_WINDOW_END - REG_DATA_WINDOW_START + 1;

pub const OTA_CMD_START: u16 = 1;
pub const OTA_CMD_COMMIT: u16 = 2;
pub const OTA_CMD_ABORT: u16 = 3;

// oTA status values read back from REG_OTA_CONTROL
pub const OTA_STATUS_IDLE: u16 = 0;
pub const OTA_STATUS_RECEIVING: u16 = 1;
pub const OTA_STATUS_VALIDATING: u16 = 2;
pub const OTA_STATUS_SUCCESS: u16 = 3;
pub const OTA_STATUS_ERROR: u16 = 4;

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


pub fn crc32(data: &[u8]) -> u32 {
    // standard reflected CRC32
    let mut v: u32 = 0xFFFF_FFFF;
    for &x in data {
        v ^= u32::from(x);
        for _ in 0..8 {
            if v & 1 != 0 {
                v = (v >> 1) ^ 0xEDB8_8320;
            } else {
                v >>= 1;
            }
        }
    }
    !v
}

#[cfg(test)]
mod tests {
    use super::*;

    // sanity: empty input should give CRC32 of nothing (0x00000000)
    #[test]
    fn crc32_empty() {
        assert_eq!(crc32(&[]), 0x0000_0000);
    }

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
