// gateway enrollment + access token claim against the cloud registry.

use crate::config::RegistryConfig;
use reqwest::Client;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct GatewayCredentials {
    pub gateway_id: String,
    pub access_token: Option<String>,
}

#[derive(Serialize)]
struct EnrollReq<'a> {
    gateway_id: &'a str,
    enrollment_token: &'a str,
}

#[derive(serde::Deserialize)]
struct ClaimResp {
    access_token: String,
}

/// register this gateway
pub async fn request_enrollment(
    cfg: &RegistryConfig,
    gw_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let u = format!("{}/provisioning/gateways", cfg.url);
    let b = EnrollReq {
        gateway_id: gw_id,
        enrollment_token: &cfg.enrollment_token,
    };

    Client::new()
        .post(&u)
        .json(&b)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

/// claim an access token
pub async fn claim_access_token(
    cfg: &RegistryConfig,
    gw_id: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let u = format!("{}/provisioning/gateways/{gw_id}/claim", cfg.url);

    let r: ClaimResp = Client::new()
        .post(&u)
        .send().await?
        .error_for_status()?
        .json().await?;

    Ok(r.access_token)
}
