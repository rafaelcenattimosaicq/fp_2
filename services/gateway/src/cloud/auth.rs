// oAuth2 client_credentials 
use base64::Engine;
use serde::Deserialize;
use std::time::{Duration, Instant};


#[derive(Debug, Clone)]
pub struct CloudConfig {
    pub client_id: String,
    pub client_secret: String,
    pub token_url: String,
    pub policies_api_url: String,
}


const TOKEN_EXPIRY_BUFFER: u64 = 60;

#[derive(Debug, Deserialize)]
struct CognitoTokenResp {
    access_token: String,
    expires_in: u64,
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
            exp: Instant::now(),
            http: reqwest::Client::new(),
        }
    }

    pub const fn http_client(&self) -> &reqwest::Client { &self.http }


    pub async fn get_token(&mut self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref t) = self.cached_tok {
            if Instant::now() < self.exp { return Ok(t.clone()); }
        }

        let s = format!("{}:{}", self.cfg.client_id, self.cfg.client_secret);
        let b = base64::engine::general_purpose::STANDARD.encode(&s);

        let r = self.http
            .post(&self.cfg.token_url)
            .header("Authorization", format!("Basic {b}"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body("grant_type=client_credentials&scope=policies-api/read")
            .send()
            .await?;

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

    #[test]
    fn new_manager_has_no_token() {
        let mgr = TokenManager::new(test_cfg());
        assert!(mgr.cached_tok.is_none());
        assert!(mgr.exp <= Instant::now());
    }


    #[tokio::test]
    async fn returns_cached_when_still_valid() {
        let mut mgr = TokenManager::new(test_cfg());
        mgr.cached_tok = Some("eyJhbGciOiJSUzI1NiIsInR5cCI6Ikp".into());
        mgr.exp = Instant::now() + Duration::from_secs(3600);

        let tok = mgr.get_token().await.expect("should return cached");
        assert_eq!(tok, "eyJhbGciOiJSUzI1NiIsInR5cCI6Ikp");
    }
}
