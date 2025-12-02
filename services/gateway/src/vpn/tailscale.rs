use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TailscaleState {
    NotInstalled,
    Stopped,
    Connected(String),
}

#[derive(Debug, thiserror::Error)]
pub enum TailscaleError {
    #[error("install: {0}")]
    Install(String),
    #[error("connect: {0}")]
    Connect(String),
}

pub fn check_status() -> TailscaleState {
    let Ok(out) = Command::new("tailscale").args(["status", "--json"]).output() else { return TailscaleState::NotInstalled };
    if !out.status.success() {
        return TailscaleState::Stopped;
    }
    extract_v4_ip(&String::from_utf8_lossy(&out.stdout))
        .map_or(TailscaleState::Stopped, TailscaleState::Connected)
}

// pull the 100.x.y.z CGNAT address out of `tailscale status --json`.
// tailscale also assigns an fd7a:: v6 address but we only care about v4
// because that's what the coordinator uses for worker registration.
fn extract_v4_ip(raw: &str) -> Option<String> {
    let j: serde_json::Value = serde_json::from_str(raw).ok()?;
    j.get("Self")?
        .get("TailscaleIPs")?
        .as_array()?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .find(|x| x.starts_with("100."))
        .map(String::from)
}

pub fn install() -> Result<(), TailscaleError> {
    // linux: official one-liner from tailscale.com
    // macos: homebrew cask (for local dev only, production is always linux)
    let out = if cfg!(target_os = "linux") {
        Command::new("sh")
            .args(["-c", "curl -fsSL https://tailscale.com/install.sh | sudo sh"])
            .output()
    } else if cfg!(target_os = "macos") {
        Command::new("brew").args(["install", "--cask", "tailscale"]).output()
    } else {
        return Err(TailscaleError::Install("unsupported OS".into()));
    };

    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(TailscaleError::Install(
            String::from_utf8_lossy(&o.stderr).trim().to_string(),
        )),
        Err(e) => Err(TailscaleError::Install(e.to_string())),
    }
}

pub fn connect(auth_key: &str, hostname: &str) -> Result<String, TailscaleError> {
    let r = Command::new("sudo")
        .args([
            "tailscale", "up",
            &format!("--authkey={auth_key}"),
            &format!("--hostname={hostname}"),
            "--accept-routes",
            // don't let tailscale touch /etc/resolv.conf.
            // the Pi's local dnsmasq handles Cloud Map names (*.iot.local)
            // and tailscale's MagicDNS would shadow them.
            "--accept-dns=false",
            "--reset",
            "--timeout=30s",
        ])
        .output()
        .map_err(|e| TailscaleError::Connect(e.to_string()))?;

    if !r.status.success() {
        let s = String::from_utf8_lossy(&r.stderr);
        return Err(TailscaleError::Connect(s.trim().to_string()));
    }

    // sometimes `tailscale up` exits 0 before the backend has fully
    // transitioned to Running. seen this on Pi 3B+ with slow SD cards.
    // we just check and warn rather than failing because it usually
    // sorts itself out within a second or two.
    if let Ok(tmp) = Command::new("tailscale").args(["status", "--json"]).output() {
        let raw = String::from_utf8_lossy(&tmp.stdout);
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            let _x = v.get("BackendState").and_then(|s| s.as_str());
        }
    }

    match check_status() {
        TailscaleState::Connected(ip) => Ok(ip),
        other => Err(TailscaleError::Connect(
            format!("tailscale up succeeded but status is {other:?}")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_v4_from_status_json() {
        let blob = r#"{"Self":{"TailscaleIPs":["100.88.85.75","fd7a:115c:a1e0::1"]}}"#;
        assert_eq!(extract_v4_ip(blob), Some("100.88.85.75".into()));
    }

    #[test]
    fn v6_only_returns_none() {
        let blob = r#"{"Self":{"TailscaleIPs":["fd7a:115c:a1e0::1"]}}"#;
        assert_eq!(extract_v4_ip(blob), None);
    }

    // discovered this when a containerised CI runner returned "{}" from
    // tailscale status. the old code would panic on the unwrap chain.
    #[test]
    fn empty_json_is_none() {
        assert_eq!(extract_v4_ip("{}"), None);
        assert_eq!(extract_v4_ip("not json"), None);
    }
}
