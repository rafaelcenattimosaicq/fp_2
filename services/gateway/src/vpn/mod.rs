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

#[allow(unused)]
const STALE_AUTH_KEY_SECS: u64 = 21000;

const POLL_INTERVAL: Duration = Duration::from_secs(5);

// 6h * 3600 / 5s = 4320 polls 
const MAX_POLLS: u32 = 4320;

pub async fn connect_vpn(
    cfg: &VpnConfig,
    gw_id: &str,
    state: &SharedState,
) -> Result<VpnEndpoints, Box<dyn std::error::Error + Send + Sync>> {
    set_status(state, VpnStatus::Checking);

    let s = tailscale::check_status();

    if let tailscale::TailscaleState::Connected(ref v) = s {
        set_status(state, VpnStatus::Provisioning);
        let r = do_provision(cfg, gw_id, state).await?;
        set_status(state, VpnStatus::Connected(v.clone()));
        return Ok(endpoints_from(v.clone(), &r));
    }

    if s == tailscale::TailscaleState::NotInstalled {
        set_status(state, VpnStatus::Installing);
        push(state, "installing tailscale...");
        tailscale::install().inspect_err(|e| {
            set_status(state, VpnStatus::Error(e.to_string()));
        })?;
    }

    set_status(state, VpnStatus::Provisioning);
    let r = do_provision(cfg, gw_id, state).await?;

    set_status(state, VpnStatus::Connecting);
    push(state, "connecting to tailscale...");
    let x = tailscale::connect(&r.auth_key, gw_id).inspect_err(|e| {
        set_status(state, VpnStatus::Error(e.to_string()));
    })?;

    set_status(state, VpnStatus::Connected(x.clone()));
    push(state, &format!("VPN up: {x} on {}", r.tailnet));

    Ok(endpoints_from(x, &r))
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
    let data = fingerprint::HardwareFingerprint::collect();

    // the setup tab lets field techs paste a secret at runtime,
    // which ends up in shared state. if they haven't pasted one yet
    // we fall back to whatever's in gateway.yaml (if anything).
    let sec = state.read().ok()
        .map(|s| s.vpn_secret.clone())
        .filter(|s| !s.is_empty() && s != "${GATEWAY_SECRET}")
        .or_else(|| cfg.pre_shared_secret.clone());

    push(state, "submitting VPN request...");
    let tok = provisioner::submit_vpn_request(
        &cfg.provisioner_url, gw_id, sec.as_deref(), &data,
    ).await?;

    push(state, "waiting for admin approval in Cloud Desktop...");

    let mut i = 0u32;
    loop {
        tokio::time::sleep(POLL_INTERVAL).await;
        i += 1;
        if i > MAX_POLLS {
            let tmp = "timed out waiting for VPN approval (auth key likely expired)";
            set_status(state, VpnStatus::Error(tmp.into()));
            return Err(tmp.into());
        }

        match provisioner::poll_vpn_status(&cfg.provisioner_url, &tok).await {
            Ok(provisioner::PollResult::Approved(x)) => return Ok(x),
            Ok(provisioner::PollResult::Pending) => {}
            Ok(provisioner::PollResult::Denied) => {
                set_status(state, VpnStatus::Error("request denied".into()));
                return Err("VPN request denied by admin".into());
            }
            Err(_e) => {}
        }
    }
}

// ---this is to remove the state.write().unwrap() noise ------------

fn set_status(state: &SharedState, s: VpnStatus) {
    state.write().expect("vpn lock").vpn_status = s;
}

fn push(state: &SharedState, msg: &str) {
    if let Ok(mut s) = state.write() {
        s.push_log(LogLevel::Info, msg);
    }
}
