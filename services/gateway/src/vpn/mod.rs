pub mod tailscale;
pub mod provisioner;
pub mod fingerprint;

use crate::config::VpnConfig;
use crate::state::{LogLevel, SharedState, VpnStatus};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct VpnEndpoints {
    pub local_ip: String,
    pub coordinator_host: String,
    #[allow(dead_code, reason = "stored from provisioner response for future gRPC client integration")]
    pub coordinator_grpc_port: u16,
    pub coordinator_rest_port: u16,
    pub mqtt_broker_host: Option<String>,
    pub mqtt_broker_port: u16,
}

const POLL_INTERVAL: Duration = Duration::from_secs(5);

// 6h * 3600 / 5s = 4320 polls before the tailscale auth key expires.
// the free-tier API won't let you request keys longer than 6h,
// and we've seen the key actually die at ~5h50m on one occasion,
// but 4320 is close enough.
const MAX_POLLS: u32 = 4320;

pub async fn connect_vpn(
    cfg: &VpnConfig,
    gw_id: &str,
    state: &SharedState,
) -> Result<VpnEndpoints, Box<dyn std::error::Error + Send + Sync>> {
    set_status(state, VpnStatus::Checking);

    let ts = tailscale::check_status();

    if let tailscale::TailscaleState::Connected(ref ip) = ts {
        tracing::info!(%ip, "tailscale already connected, skipping install");
        set_status(state, VpnStatus::Provisioning);
        let pr = do_provision(cfg, gw_id, state).await?;
        set_status(state, VpnStatus::Connected(ip.clone()));
        return Ok(endpoints_from(ip.clone(), &pr));
    }

    if ts == tailscale::TailscaleState::NotInstalled {
        set_status(state, VpnStatus::Installing);
        push(state, "installing tailscale...");
        tailscale::install().inspect_err(|e| {
            set_status(state, VpnStatus::Error(e.to_string()));
        })?;
    }

    set_status(state, VpnStatus::Provisioning);
    let pr = do_provision(cfg, gw_id, state).await?;

    set_status(state, VpnStatus::Connecting);
    push(state, "connecting to tailscale...");
    let ip = tailscale::connect(&pr.auth_key, gw_id).inspect_err(|e| {
        set_status(state, VpnStatus::Error(e.to_string()));
    })?;

    set_status(state, VpnStatus::Connected(ip.clone()));
    push(state, &format!("VPN up: {ip} on {}", pr.tailnet));

    Ok(endpoints_from(ip, &pr))
}

fn endpoints_from(ip: String, pr: &provisioner::ProvisionResponse) -> VpnEndpoints {
    VpnEndpoints {
        local_ip: ip,
        coordinator_host: pr.coordinator_host.clone(),
        coordinator_grpc_port: pr.coordinator_grpc_port,
        coordinator_rest_port: pr.coordinator_rest_port,
        mqtt_broker_host: pr.mqtt_broker_host.clone(),
        mqtt_broker_port: pr.mqtt_broker_port,
    }
}

async fn do_provision(
    cfg: &VpnConfig,
    gw_id: &str,
    state: &SharedState,
) -> Result<provisioner::ProvisionResponse, Box<dyn std::error::Error + Send + Sync>> {
    let fp = fingerprint::HardwareFingerprint::collect();
    tracing::info!(mac = %fp.mac_address, cpu = %fp.cpu_id, "fingerprint collected");

    // the setup tab lets field techs paste a secret at runtime,
    // which ends up in shared state. if they haven't pasted one yet
    // we fall back to whatever's in gateway.yaml (if anything).
    let secret = state.read().ok()
        .map(|s| s.vpn_secret.clone())
        .filter(|s| !s.is_empty() && s != "${GATEWAY_SECRET}")
        .or_else(|| cfg.pre_shared_secret.clone());

    push(state, "submitting VPN request...");
    let token = provisioner::submit_vpn_request(
        &cfg.provisioner_url, gw_id, secret.as_deref(), &fp,
    ).await?;

    push(state, "waiting for admin approval in Cloud Desktop...");

    let mut n = 0u32;
    loop {
        tokio::time::sleep(POLL_INTERVAL).await;
        n += 1;
        if n > MAX_POLLS {
            let msg = "timed out waiting for VPN approval (auth key likely expired)";
            set_status(state, VpnStatus::Error(msg.into()));
            return Err(msg.into());
        }

        match provisioner::poll_vpn_status(&cfg.provisioner_url, &token).await {
            Ok(provisioner::PollResult::Approved(pr)) => return Ok(pr),
            Ok(provisioner::PollResult::Pending) => {}
            Ok(provisioner::PollResult::Denied) => {
                set_status(state, VpnStatus::Error("request denied".into()));
                return Err("VPN request denied by admin".into());
            }
            // network blip, lambda timeout, etc. just try again.
            Err(e) => tracing::warn!(%e, "poll failed, retrying"),
        }
    }
}

// ---- tiny helpers to cut down on the state.write().unwrap() noise ----

fn set_status(state: &SharedState, s: VpnStatus) {
    state.write().expect("vpn lock").vpn_status = s;
}

fn push(state: &SharedState, msg: &str) {
    if let Ok(mut s) = state.write() {
        s.push_log(LogLevel::Info, msg);
    }
}
