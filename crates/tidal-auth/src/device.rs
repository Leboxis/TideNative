//! Flux de secours : OAuth 2.0 Device Authorization Grant (RFC 8628).
//!
//! ⚠️ D'après les commentaires du projet OxiTide/hiresTI, TIDAL plafonne les
//! tokens issus de ce flux à l'AAC 320 kbps depuis ~avril 2026 : à proposer
//! uniquement comme « connexion rapide, qualité limitée ».

use serde::Deserialize;
use std::time::Duration;

use crate::credentials::ClientCredentials;
use crate::error::{AuthError, Result};
use crate::token::{OAuthError, TokenSet, post_token, push_secret};
use crate::{AUTH_BASE, SCOPE};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAuthorization {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_interval() -> u64 {
    2
}

impl DeviceAuthorization {
    /// URL à ouvrir dans le navigateur (TIDAL omet souvent le schéma).
    pub fn browser_url(&self) -> String {
        let raw = self.verification_uri_complete.as_deref().unwrap_or(&self.verification_uri);
        if raw.starts_with("http") { raw.to_owned() } else { format!("https://{raw}") }
    }
}

pub async fn start(http: &reqwest::Client, creds: &ClientCredentials) -> Result<DeviceAuthorization> {
    let pair = creds.pair(false)?;
    let resp = http
        .post(format!("{AUTH_BASE}/device_authorization"))
        .form(&[("client_id", pair.id), ("scope", SCOPE)])
        .send()
        .await?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        return Err(AuthError::Api { status: status.as_u16(), body });
    }
    Ok(serde_json::from_str(&body)?)
}

/// Interroge `/token` toutes les `interval` secondes jusqu'à validation,
/// refus ou expiration du code.
pub async fn poll(http: &reqwest::Client, creds: &ClientCredentials, auth: &DeviceAuthorization) -> Result<TokenSet> {
    let pair = creds.pair(false)?;
    let mut form = vec![
        ("client_id", pair.id),
        ("device_code", auth.device_code.as_str()),
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ("scope", SCOPE),
    ];
    push_secret(&mut form, &pair);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(auth.expires_in);
    let mut interval = auth.interval.max(1);

    loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        if tokio::time::Instant::now() >= deadline {
            return Err(AuthError::DeviceExpired);
        }
        match post_token(http, &form).await {
            Ok(resp) => return resp.into_token_set(false, None),
            Err(AuthError::Api { status: 400 | 401, body }) => {
                let code = serde_json::from_str::<OAuthError>(&body).map(|e| e.error).unwrap_or_default();
                match code.as_str() {
                    "authorization_pending" => continue,
                    "slow_down" => interval += 5,
                    "expired_token" => return Err(AuthError::DeviceExpired),
                    "access_denied" => return Err(AuthError::AccessDenied),
                    _ => return Err(AuthError::Api { status: 400, body }),
                }
            }
            Err(e) => return Err(e),
        }
    }
}
