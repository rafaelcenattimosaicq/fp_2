// oAuth2 client_credentials flow against our Cognito user pool.
// this was built for the cloud-sync feature that ended up getting cut
// before the April demo. The CloudConfig struct used to live in config.rs
// but got removed when we descoped; I'm re-defining it here so the module
// at least compiles on its own. Ugly, but beats a circular refactor of
// dead code.
//
// token refresh logic: Cognito hands back a JWT with an expires_in field
// (usually 3600s). We shave 60s off to avoid a race where the token
// expires mid-request, seen this happen on the the factory floor
// when the gateway's NTP was drifting by ~45 seconds.

use base64::Engine;
use serde::Deserialize;
use std::time::{Duration, Instant};

// pulled out of config.rs when cloud sync was axed. Duplicated here so
// the module doesn't depend on a type that no longer exists upstream.
#[derive(Debug, Clone)]
pub struct CloudConfig {
    pub client_id: String,
    pub client_secret: String,
    pub token_url: String,
    pub policies_api_url: String,
}

// 60s margin. 30s wasn't enough, we had a token expire between the
// gET /policies list call and the GET /policies/{name} detail call
// because the lambda cold start ate 20+ seconds on the first invocation.
const TOKEN_EXPIRY_BUFFER: u64 = 60;

#[derive(Debug, Deserialize)]
struct CognitoTokenResp {
    access_token: String,
    expires_in: u64,
    // token_type is always "Bearer" but Cognito sends it anyway
    #[allow(dead_code)] // serde needs it in the struct for deserialization to not choke
    token_type: Option<String>,
}

pub struct TokenManager {
    cfg: CloudConfig,
    cached_tok: Option<String>,
    exp: Instant,
    http: reqwest::Client,
}

impl TokenManager {
    pub fn new(cfg: CloudConfig) -> Self {
        Self {
            cfg,
            cached_tok: None,
            // start expired so the first get_token() forces a fetch
            exp: Instant::now(),
            http: reqwest::Client::new(),
        }
    }

    pub const fn http_client(&self) -> &reqwest::Client { &self.http }

    /// returns a valid Bearer token, refreshing from Cognito if the cached one
    /// is stale. Callers shouldn't cache this themselves, just call get_token()
    /// before each API request.
    pub async fn get_token(&mut self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // fast path: token still valid
        if let Some(ref t) = self.cached_tok {
            if Instant::now() < self.exp { return Ok(t.clone()); }
        }

        // slow path: hit Cognito
        let s = format!("{}:{}", self.cfg.client_id, self.cfg.client_secret);
        let b = base64::engine::general_purpose::STANDARD.encode(&s);

        let r = self.http
            .post(&self.cfg.token_url)
            .header("Authorization", format!("Basic {b}"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            // scope must match the resource server configured in Cognito
            .body("grant_type=client_credentials&scope=policies-api/read")
            .send()
            .await?;

        // cognito returns 400 for bad credentials, not 401, which is annoying
        // to debug because reqwest's error_for_status message is vague.
        if !r.status().is_success() {
            let st = r.status();
            let tmp = r.text().await.unwrap_or_default();
            return Err(format!(
                "cognito token request failed ({}): {}",
                st, tmp
            ).into());
        }

        let data: CognitoTokenResp = r.json().await?;

        let v = data.expires_in.saturating_sub(TOKEN_EXPIRY_BUFFER);
        self.exp = Instant::now() + Duration::from_secs(v);
        self.cached_tok = Some(data.access_token.clone());

        Ok(data.access_token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cfg() -> CloudConfig {
        CloudConfig {
            client_id: "test-client-id".into(),
            client_secret: "test-client-secret".into(),
            token_url: "https://cognito.us-east-1.amazonaws.com/oauth2/token".into(),
            policies_api_url: "https://api.gateway.example.com".into(),
        }
    }

    // fresh manager should have no cached token, first call must
    // always go to Cognito
    #[test]
    fn new_manager_has_no_token() {
        let mgr = TokenManager::new(test_cfg());
        assert!(mgr.cached_tok.is_none());
        // expiry in the past forces a fetch
        assert!(mgr.exp <= Instant::now());
    }

    // simulates a scenario where we already got a token and it hasn't expired yet.
    // this is the normal steady-state on the gateway, token lasts an hour,
    // we poll every 5 seconds.
    #[tokio::test]
    async fn returns_cached_when_still_valid() {
        let mut mgr = TokenManager::new(test_cfg());
        mgr.cached_tok = Some("eyJhbGciOiJSUzI1NiIsInR5cCI6Ikp".into());
        mgr.exp = Instant::now() + Duration::from_secs(3600);

        let tok = mgr.get_token().await.expect("should return cached");
        assert_eq!(tok, "eyJhbGciOiJSUzI1NiIsInR5cCI6Ikp");
    }
}
