//! Jeu de tokens, échange auprès de `/oauth2/token` et renouvellement.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::credentials::{ClientCredentials, ClientPair};
use crate::error::{AuthError, Result};
use crate::{AUTH_BASE, SCOPE};

/// Marge avant expiration à partir de laquelle on renouvelle (secondes).
const EXPIRY_SKEW_SECS: u64 = 120;

/// Identifiant client OAuth PKCE public du lecteur web officiel `listen.tidal.com` (`cid = 8049`).
/// Permet le renouvellement automatique des sessions capturées via WebView2 sans configuration externe.
pub const WEB_PLAYER_PKCE_CLIENT_ID: &str = "49YxDN9a2aFV6RTG";
pub const WEB_PLAYER_SCOPE: &str = "r_usr w_usr";

fn extract_jwt_exp(jwt: &str) -> Option<u64> {
    let payload_b64 = jwt.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    let val: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    val.get("exp")?.as_u64()
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub token_type: String,
    pub access_token: String,
    pub refresh_token: String,
    /// Expiration de l'access_token, en secondes Unix.
    pub expires_at: u64,
    /// Token obtenu par PKCE (Hi-Res) ou par Device Code.
    pub is_pkce: bool,
    /// Renseignés après `GET /v1/sessions` (permet un démarrage sans réseau).
    #[serde(default)]
    pub user_id: Option<u64>,
    #[serde(default)]
    pub country_code: Option<String>,
}

impl std::fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenSet")
            .field("expires_at", &self.expires_at)
            .field("is_pkce", &self.is_pkce)
            .field("user_id", &self.user_id)
            .finish_non_exhaustive()
    }
}

impl TokenSet {
    /// Construit une session authentifiée capturée directement depuis la fenêtre web TIDAL.
    pub fn from_web_session(
        access_token: String,
        refresh_token: Option<String>,
        expires_in: Option<u64>,
    ) -> Self {
        let expires_at = extract_jwt_exp(&access_token)
            .or_else(|| expires_in.map(|secs| now_secs() + secs))
            .unwrap_or_else(|| now_secs() + 14_400);
        Self {
            token_type: "Bearer".into(),
            access_token,
            refresh_token: refresh_token.unwrap_or_default(),
            expires_at,
            is_pkce: true,
            user_id: None,
            country_code: None,
        }
    }

    pub fn needs_refresh(&self) -> bool {
        !self.refresh_token.is_empty() && now_secs() + EXPIRY_SKEW_SECS >= self.expires_at
    }

    pub fn authorization(&self) -> String {
        format!("{} {}", self.token_type, self.access_token)
    }
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[derive(Deserialize)]
pub(crate) struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
    pub expires_in: u64,
}

impl TokenResponse {
    pub(crate) fn into_token_set(self, is_pkce: bool, previous_refresh: Option<&str>) -> Result<TokenSet> {
        let refresh_token = self
            .refresh_token
            .or_else(|| previous_refresh.map(str::to_owned))
            .ok_or(AuthError::NoRefreshToken)?;
        Ok(TokenSet {
            token_type: self.token_type.unwrap_or_else(|| "Bearer".into()),
            access_token: self.access_token,
            refresh_token,
            expires_at: now_secs() + self.expires_in,
            is_pkce,
            user_id: None,
            country_code: None,
        })
    }
}

/// Erreur OAuth normalisée renvoyée par `auth.tidal.com`.
#[derive(Debug, Deserialize)]
pub(crate) struct OAuthError {
    #[serde(default)]
    pub error: String,
}

/// POST `application/x-www-form-urlencoded` sur `/oauth2/token`.
/// Renvoie `Err(Api{..})` avec le corps brut en cas de statut non 2xx.
pub(crate) async fn post_token(http: &reqwest::Client, form: &[(&str, &str)]) -> Result<TokenResponse> {
    let resp = http.post(format!("{AUTH_BASE}/token")).form(form).send().await?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        return Err(AuthError::Api { status: status.as_u16(), body });
    }
    Ok(serde_json::from_str(&body)?)
}

pub(crate) fn push_secret<'a>(form: &mut Vec<(&'a str, &'a str)>, pair: &ClientPair<'a>) {
    if let Some(secret) = pair.secret {
        form.push(("client_secret", secret));
    }
}

/// `grant_type=refresh_token` avec la paire d'identifiants du flux d'origine
/// (ou le client PKCE officiel de `listen.tidal.com` pour les sessions WebView2).
pub async fn refresh(http: &reqwest::Client, creds: &ClientCredentials, current: &TokenSet) -> Result<TokenSet> {
    let resp = match creds.pair(current.is_pkce) {
        Ok(pair) => {
            let mut form = vec![
                ("grant_type", "refresh_token"),
                ("refresh_token", current.refresh_token.as_str()),
                ("client_id", pair.id),
                ("scope", SCOPE),
            ];
            push_secret(&mut form, &pair);
            match post_token(http, &form).await {
                Ok(r) => r,
                Err(_) if current.is_pkce => {
                    let web_form = [
                        ("grant_type", "refresh_token"),
                        ("refresh_token", current.refresh_token.as_str()),
                        ("client_id", WEB_PLAYER_PKCE_CLIENT_ID),
                        ("scope", WEB_PLAYER_SCOPE),
                    ];
                    post_token(http, &web_form).await?
                }
                Err(e) => return Err(e),
            }
        }
        Err(_) if current.is_pkce => {
            let web_form = [
                ("grant_type", "refresh_token"),
                ("refresh_token", current.refresh_token.as_str()),
                ("client_id", WEB_PLAYER_PKCE_CLIENT_ID),
                ("scope", WEB_PLAYER_SCOPE),
            ];
            post_token(http, &web_form).await?
        }
        Err(e) => return Err(e),
    };
    let mut next = resp.into_token_set(current.is_pkce, Some(&current.refresh_token))?;
    if let Some(jwt_exp) = extract_jwt_exp(&next.access_token) {
        next.expires_at = jwt_exp;
    }
    next.user_id = current.user_id;
    next.country_code = current.country_code.clone();
    Ok(next)
}
