use super::fingerprint::HardwareFingerprint;
use std::time::Duration;

// provisioner response has grown over time, originally it was just the auth key
// and the tailnet name. coordinator_host/ports were added when we moved the
// coordinator off a static EC2 to Fargate (IPs change on every deploy).
// mqtt_broker_host came even later when we started running the broker on a
// separate task instead of sidecar.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProvisionResponse {
    pub auth_key: String,
    pub tailnet: String,
    pub coordinator_host: String,
    pub coordinator_grpc_port: u16,
    pub coordinator_rest_port: u16,
    #[serde(default)]
    pub mqtt_broker_host: Option<String>,
    #[serde(default = "default_mqtt_port")]
    pub mqtt_broker_port: u16,
}

const fn default_mqtt_port() -> u16 { 1883 }

#[derive(Debug)]
pub enum PollResult {
    Approved(ProvisionResponse),
    Pending,
    Denied,
}

#[derive(Debug, thiserror::Error)]
pub enum ProvisionError {
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{status}: {body}")]
    Api { status: u16, body: String },
}

// kept separate from poll because submit needs to build the JSON body with
// the fingerprint + optional secret, and the error handling is different
// (submit failing is fatal, poll failing just means "try again later")
pub async fn submit_vpn_request(
    base_url: &str,
    gw_id: &str,
    secret: Option<&str>,
    fp: &HardwareFingerprint,
) -> Result<String, ProvisionError> {
    let u = format!("{}/vpn/request", base_url.trim_end_matches('/'));

    let mut data = serde_json::json!({
        "gateway_id": gw_id,
        "fingerprint": fp,
    });
    if let Some(s) = secret {
        data["pre_shared_secret"] = s.into();
    }

    // lambda cold-start behind API GW is ~8s worst case.
    // 10s timeout caused intermittent failures on the Joinville pilot,
    // bumped to 15 after that.
    let r = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?
        .post(&u)
        .json(&data)
        .send()
        .await?;

    if !r.status().is_success() {
        let c = r.status().as_u16();
        let b = r.text().await.unwrap_or_else(|_| "(empty)".into());
        return Err(ProvisionError::Api { status: c, body: b });
    }

    let v: serde_json::Value = r.json().await?;
    v["request_token"].as_str().map_or_else(|| Err(ProvisionError::Api {
        status: 200,
        body: "response missing request_token".into(),
    }), |t| {
        Ok(t.to_owned())
    })
}

pub async fn poll_vpn_status(
    base_url: &str,
    token: &str,
) -> Result<PollResult, ProvisionError> {
    let u = format!("{}/vpn/poll/{token}", base_url.trim_end_matches('/'));

    // poll can use a shorter timeout, if the lambda is warm it responds in <1s,
    // and if it's cold we'll just retry on the next 5s cycle anyway
    let r = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()?
        .get(&u)
        .send()
        .await?;

    let c = r.status();
    if !c.is_success() {
        return Err(ProvisionError::Api {
            status: c.as_u16(),
            body: r.text().await.unwrap_or_default(),
        });
    }

    let v: serde_json::Value = r.json().await?;

    // older provisioner lambda (pre-march 2026) sometimes omits "status"
    // entirely for pending requests. treat missing as pending.
    match v["status"].as_str() {
        Some("approved") => {
            let x: ProvisionResponse = serde_json::from_value(v).map_err(|e| {
                ProvisionError::Api { status: 200, body: e.to_string() }
            })?;
            Ok(PollResult::Approved(x))
        }
        Some("denied") => Ok(PollResult::Denied),
        _ => Ok(PollResult::Pending),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spins up a throwaway TCP server that accepts one request and sends
    // back a canned JSON response. janky but it works and doesn't need
    // an HTTP framework as a dev dependency.
    #[tokio::test]
    async fn submit_includes_secret_and_fingerprint() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let srv = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let n = tokio::io::AsyncReadExt::read(&mut sock, &mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();

            let body = r#"{"request_token":"tok-9f3a","status":"pending"}"#;
            let http = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len(),
            );
            tokio::io::AsyncWriteExt::write_all(&mut sock, http.as_bytes())
                .await
                .unwrap();
            req
        });

        let fp = HardwareFingerprint {
            mac_address: "dc:a6:32:12:34:56".into(),
            cpu_id: "10000000e4b3f592".into(),
            hostname: "gw-edge-001".into(),
            os_info: "Linux 5.15.84-v8+ aarch64".into(),
            board_serial: "100000004a5d8e3c".into(),
        };

        let tok = submit_vpn_request(
            &format!("http://127.0.0.1:{port}"),
            "GW-EDGE-001",
            Some("OuzMZ9ocKwZt9uN8"),
            &fp,
        )
        .await
        .expect("submit failed");

        assert_eq!(tok, "tok-9f3a");

        let raw = srv.await.unwrap();
        assert!(raw.contains("GW-EDGE-001"));
        assert!(raw.contains("pre_shared_secret"));
        assert!(raw.contains("dc:a6:32"));
        // secret goes in the body, not as a Bearer header
        assert!(!raw.contains("Authorization"));
    }
}
