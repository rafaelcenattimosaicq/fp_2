use std::process::Command;

#[derive(Debug, Clone, serde::Serialize)]
pub struct HardwareFingerprint {
    pub mac_address: String,
    pub cpu_id: String,
    pub hostname: String,
    pub os_info: String,
    pub board_serial: String,
}

impl HardwareFingerprint {
    /// grab whatever hardware identy
    pub fn collect() -> Self {
        let m = read_primary_mac();
        let c = read_cpu_serial();
        let h = Command::new("hostname")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|x| !x.is_empty())
            .unwrap_or_else(gethostname_fallback);

        let os = Command::new("uname")
            .arg("-a")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();

        let bs = read_board_serial();

        Self { mac_address: m, cpu_id: c, hostname: h, os_info: os, board_serial: bs }
    }
}

fn read_primary_mac() -> String {
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        let Ok(entries) = fs::read_dir("/sys/class/net") else {
            return "n/a".into();
        };

        for entry in entries.flatten() {
            let iface = entry.file_name().to_string_lossy().to_string();
            if iface == "lo" || iface.starts_with("veth") || iface.starts_with("br-")
                || iface.starts_with("docker") || iface.starts_with("tailscale")
            {
                continue;
            }
            let path = format!("/sys/class/net/{iface}/address");
            if let Ok(raw) = fs::read_to_string(&path) {
                let mac = raw.trim();
                if !mac.is_empty() && mac != "00:00:00:00:00:00" {
                    return mac.to_string();
                }
            }
        }
        "n/a".into()
    }

    #[cfg(not(target_os = "linux"))]
    {
        Command::new("ifconfig")
            .output()
            .ok()
            .and_then(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .find(|l| l.contains("ether "))
                    .map(|l| {
                        l.trim()
                            .strip_prefix("ether ")
                            .unwrap_or("n/a")
                            .split_whitespace()
                            .next()
                            .unwrap_or("n/a")
                            .to_string()
                    })
            })
            .unwrap_or_else(|| "n/a".into())
    }
}

#[allow(clippy::missing_const_for_fn, reason = "non-linux cfg branch returns String::new() but cfg(linux) branch is not const")]
fn read_cpu_serial() -> String {
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        let Ok(info) = fs::read_to_string("/proc/cpuinfo") else {
            return String::new();
        };
        for line in info.lines() {
            if let Some(rest) = line.strip_prefix("Serial") {
                if let Some(val) = rest.split(':').nth(1) {
                    let trimmed = val.trim();
                    if !trimmed.is_empty() && trimmed != "0000000000000000" {
                        return trimmed.to_string();
                    }
                }
            }
        }
        String::new()
    }

    #[cfg(not(target_os = "linux"))]
    { String::new() }
}

#[allow(clippy::missing_const_for_fn, reason = "non-linux cfg branch returns String::new() but cfg(linux) branch is not const")]
fn read_board_serial() -> String {
    #[cfg(target_os = "linux")]
    {
        // devicetree path works on RPi 4 and most ARM SBCs
        std::fs::read_to_string("/sys/firmware/devicetree/base/serial-number")
            .ok()
            .map(|s| s.trim_end_matches('\0').trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_default()
    }

    #[cfg(not(target_os = "linux"))]
    { String::new() }
}

fn gethostname_fallback() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        let fp = HardwareFingerprint::collect();
        assert!(!fp.hostname.is_empty());
        assert!(!fp.os_info.is_empty());
    }

    #[test]
    fn json_round_trip() {
        let fp = HardwareFingerprint {
            mac_address: "dc:a6:32:xx:xx:xx".into(),
            cpu_id: "10000000e4b3f592".into(),
            hostname: "gw-edge-001".into(),
            os_info: "Linux 5.15.84-v8+ aarch64".into(),
            board_serial: "100000004a5d8e3c".into(),
        };

        let j = serde_json::to_value(&fp).unwrap();
        assert_eq!(j["mac_address"], "dc:a6:32:xx:xx:xx");
        assert_eq!(j["board_serial"], "100000004a5d8e3c");
    }
}
