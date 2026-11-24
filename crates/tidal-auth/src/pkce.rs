//! Flux principal : OAuth 2.0 Authorization Code + PKCE (`S256`).
//!
//! Supporte deux profils compatibles avec la fenêtre WebView2 dédiée :
//! 1. **Portail développeur TIDAL officiel (`THIRD_PARTY`)** (`developer.tidal.com`) :
//!    `redirect_uri` enregistrée par l'utilisateur (ex. `http://localhost:8989/callback`)
//!    + scopes officiels `user.read collection.read collection.write entitlements.read
//!    playback playlists.read playlists.write recommendations.read search.read search.write`.
//! 2. **Mode Android/OxiTide legacy** (`https://tidal.com/android/login/auth`) :
//!    scopes `r_usr w_usr w_sub` + `appMode=android` + `client_unique_key`.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use url::Url;

use crate::SCOPE;
use crate::credentials::ClientCredentials;
use crate::error::{AuthError, Result};
use crate::token::{TokenSet, post_token, push_secret};

pub const AUTHORIZE_URL: &str = "https://login.tidal.com/authorize";
pub const REDIRECT_URI: &str = "https://tidal.com/android/login/auth";
pub const DEFAULT_THIRD_PARTY_REDIRECT_URI: &str = "http://localhost:8989/callback";
pub const THIRD_PARTY_SCOPES: &str = "user.read collection.read collection.write entitlements.read playback playlists.read playlists.write recommendations.read search.read search.write";

/// État d'une tentative de connexion PKCE (à garder en mémoire jusqu'au retour).
pub struct PkceLogin {
    pub authorize_url: String,
    pub redirect_uri: String,
    pub scope: String,
    code_verifier: String,
    client_unique_key: Option<String>,
}

fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).map_err(|e| AuthError::Random(e.to_string()))?;
    Ok(buf)
}

impl PkceLogin {
    pub fn new(creds: &ClientCredentials) -> Result<Self> {
        let pair = creds.pair(true)?;
        // code_verifier : 32 octets aléatoires, base64url sans padding (43 car.).
        let code_verifier = URL_SAFE_NO_PAD.encode(random_bytes::<32>()?);
        // code_challenge = base64url(SHA-256(code_verifier)).
        let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));

        let redirect_uri = creds
            .redirect_uri
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(REDIRECT_URI)
            .to_string();

        let is_legacy_android = redirect_uri == REDIRECT_URI;

        let scope = creds
            .scope
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(if is_legacy_android {
                SCOPE
            } else {
                THIRD_PARTY_SCOPES
            })
            .to_string();

        let client_unique_key = if is_legacy_android {
            Some(format!("{:016x}", u64::from_le_bytes(random_bytes::<8>()?)))
        } else {
            None
        };

        let mut params: Vec<(&str, &str)> = vec![
            ("response_type", "code"),
            ("client_id", pair.id),
            ("redirect_uri", redirect_uri.as_str()),
            ("scope", scope.as_str()),
            ("code_challenge", code_challenge.as_str()),
            ("code_challenge_method", "S256"),
        ];

        if let Some(ref cuk) = client_unique_key {
            params.push(("lang", "EN"));
            params.push(("appMode", "android"));
            params.push(("client_unique_key", cuk.as_str()));
            params.push(("restrict_signup", "true"));
        }

        let url = Url::parse_with_params(AUTHORIZE_URL, &params)?;

        Ok(Self {
            authorize_url: url.into(),
            redirect_uri,
            scope,
            code_verifier,
            client_unique_key,
        })
    }

    /// Vrai si `url` correspond à la redirection attendue (à utiliser dans le handler
    /// `NavigationStarting` de WebView2 pour annuler la navigation).
    pub fn is_redirect(&self, url: &str) -> bool {
        url.starts_with(&self.redirect_uri)
    }

    /// Extrait `code` de l'URL de redirection (ou d'une chaîne de requête `?code=...`).
    pub fn extract_code(redirect_url: &str) -> Result<String> {
        let trimmed = redirect_url.trim();
        let url = Url::parse(trimmed).or_else(|_| {
            Url::parse(&format!(
                "http://localhost/callback?{}",
                trimmed.trim_start_matches('?')
            ))
        })?;
        url.query_pairs()
            .find(|(k, _)| k == "code")
            .map(|(_, v)| v.into_owned())
            .filter(|c| !c.is_empty())
            .ok_or(AuthError::MissingCode)
    }

    /// Échange le `code` contre un jeu de tokens (`is_pkce = true`).
    pub async fn finish(
        &self,
        http: &reqwest::Client,
        creds: &ClientCredentials,
        redirect_url: &str,
    ) -> Result<TokenSet> {
        let code = Self::extract_code(redirect_url)?;
        let pair = creds.pair(true)?;
        let mut form = vec![
            ("code", code.as_str()),
            ("grant_type", "authorization_code"),
            ("client_id", pair.id),
            ("redirect_uri", self.redirect_uri.as_str()),
            ("scope", self.scope.as_str()),
            ("code_verifier", self.code_verifier.as_str()),
        ];
        if let Some(ref cuk) = self.client_unique_key {
            form.push(("client_unique_key", cuk.as_str()));
        }
        push_secret(&mut form, &pair);
        post_token(http, &form).await?.into_token_set(true, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds() -> ClientCredentials {
        ClientCredentials {
            pkce_client_id: Some("test-id".into()),
            ..Default::default()
        }
    }

    #[test]
    fn authorize_url_contains_pkce_parameters() {
        let login = PkceLogin::new(&creds()).unwrap();
        let url = Url::parse(&login.authorize_url).unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q["client_id"], "test-id");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["redirect_uri"], REDIRECT_URI);
        assert_eq!(q["scope"], SCOPE);
        assert_eq!(q["client_unique_key"].len(), 16);
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(login.code_verifier.as_bytes()));
        assert_eq!(q["code_challenge"], expected);
        assert_eq!(login.code_verifier.len(), 43);
    }

    #[test]
    fn third_party_developer_portal_uses_official_scopes_and_custom_redirect() {
        let mut c = creds();
        c.redirect_uri = Some(DEFAULT_THIRD_PARTY_REDIRECT_URI.into());
        let login = PkceLogin::new(&c).unwrap();
        let url = Url::parse(&login.authorize_url).unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q["redirect_uri"], DEFAULT_THIRD_PARTY_REDIRECT_URI);
        assert_eq!(q["scope"], THIRD_PARTY_SCOPES);
        assert!(!q.contains_key("appMode"));
        assert!(!q.contains_key("client_unique_key"));
    }

    #[test]
    fn extracts_code_from_redirect() {
        let code =
            PkceLogin::extract_code("https://tidal.com/android/login/auth?code=abc123&lang=en")
                .unwrap();
        assert_eq!(code, "abc123");
        let tp =
            PkceLogin::extract_code("http://localhost:8989/callback?code=xyz789").unwrap();
        assert_eq!(tp, "xyz789");
        assert!(PkceLogin::extract_code("https://tidal.com/android/login/auth?lang=en").is_err());
    }
}
