//! Client API authentifié : refresh automatique « single-flight »,
//! rejeu sur 401, persistance du token renouvelé.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::credentials::ClientCredentials;
use crate::error::{AuthError, Result};
use crate::token::{self, TokenSet};
use crate::{API_V1, CLIENT_VERSION, OPENAPI_V2, store};

const MAX_RATE_LIMIT_RETRIES: usize = 3;
const MIN_RETRY_AFTER_MS: u64 = 100;
const MAX_RETRY_AFTER_MS: u64 = 15_000;

/// Calcule le délai d'attente pour une réponse HTTP 429 à partir de `Retry-After`
/// (secondes entières ou flottantes) avec repli exponentiel borné.
pub(crate) fn compute_rate_limit_delay(retry_after_header: Option<&str>, attempt: usize) -> Duration {
    if let Some(raw) = retry_after_header {
        let trimmed = raw.trim();
        if let Ok(secs) = trimmed.parse::<u64>() {
            let ms = secs.saturating_mul(1000).clamp(MIN_RETRY_AFTER_MS, MAX_RETRY_AFTER_MS);
            return Duration::from_millis(ms);
        }
        if let Ok(secs_f) = trimmed.parse::<f64>() {
            if secs_f.is_finite() && secs_f >= 0.0 {
                let ms = ((secs_f * 1000.0) as u64).clamp(MIN_RETRY_AFTER_MS, MAX_RETRY_AFTER_MS);
                return Duration::from_millis(ms);
            }
        }
    }
    let backoff_ms = match attempt {
        0 => 500,
        1 => 1_500,
        _ => 3_500,
    };
    Duration::from_millis(backoff_ms)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub session_id: String,
    pub user_id: u64,
    pub country_code: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackInfo {
    pub track_id: u64,
    pub audio_quality: String,
    #[serde(default)]
    pub audio_mode: Option<String>,
    pub manifest_mime_type: String,
    pub manifest: String,
    #[serde(default)]
    pub bit_depth: Option<u32>,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub track_replay_gain: Option<f32>,
    #[serde(default)]
    pub track_peak_amplitude: Option<f32>,
    /// Gain ReplayGain calculé sur l'album complet (`albumReplayGain`).
    #[serde(default)]
    pub album_replay_gain: Option<f32>,
    /// Pic maximal de l'album (`albumPeakAmplitude`), pour la protection crête.
    #[serde(default)]
    pub album_peak_amplitude: Option<f32>,
}

pub struct TidalClient {
    http: reqwest::Client,
    creds: ClientCredentials,
    token: Mutex<TokenSet>,
}

impl TidalClient {
    pub fn new(http: reqwest::Client, creds: ClientCredentials, token: TokenSet) -> Self {
        Self { http, creds, token: Mutex::new(token) }
    }

    /// Expose le pool HTTP/2 partagé pour réutiliser les connexions TLS vers le CDN audio.
    pub fn http(&self) -> reqwest::Client {
        self.http.clone()
    }

    /// Recharge le token depuis le Credential Manager (aucun appel réseau).
    pub fn from_store(http: reqwest::Client, creds: ClientCredentials) -> Result<Self> {
        let token = store::load()?.ok_or(AuthError::NoToken)?;
        Ok(Self::new(http, creds, token))
    }

    pub async fn token_snapshot(&self) -> TokenSet {
        self.token.lock().await.clone()
    }

    /// En-tête Authorization valide ; renouvelle si proche de l'expiration.
    /// Le verrou garantit qu'un seul refresh part même si 20 requêtes
    /// concurrentes (pochettes, pages…) découvrent l'expiration en même temps.
    async fn authorization(&self) -> Result<String> {
        let mut guard = self.token.lock().await;
        if guard.needs_refresh() && (self.creds.pair(guard.is_pkce).is_ok() || guard.is_pkce) {
            let next = token::refresh(&self.http, &self.creds, &guard).await?;
            store::save(&next)?;
            *guard = next;
        }
        Ok(guard.authorization())
    }

    /// Refresh forcé après un 401, sauf si un autre appel l'a déjà fait.
    async fn refresh_after_401(&self, rejected: &str) -> Result<String> {
        let mut guard = self.token.lock().await;
        if guard.authorization() == rejected
            && !guard.refresh_token.is_empty()
            && (self.creds.pair(guard.is_pkce).is_ok() || guard.is_pkce)
        {
            let next = token::refresh(&self.http, &self.creds, &guard).await?;
            store::save(&next)?;
            *guard = next;
        }
        Ok(guard.authorization())
    }

    async fn send_with_rate_limit<F>(&self, mut build: F) -> Result<reqwest::Response>
    where
        F: FnMut() -> reqwest::RequestBuilder,
    {
        for attempt in 0..=MAX_RATE_LIMIT_RETRIES {
            let resp = build().send().await?;
            if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
                && attempt < MAX_RATE_LIMIT_RETRIES
            {
                let retry_after = resp
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok());
                let delay = compute_rate_limit_delay(retry_after, attempt);
                tokio::time::sleep(delay).await;
                continue;
            }
            return Ok(resp);
        }
        unreachable!("loop always returns on final attempt")
    }

    async fn send_get(&self, url: &str, query: &[(&str, &str)], auth: &str) -> Result<String> {
        let resp = self
            .send_with_rate_limit(|| {
                self.http
                    .get(url)
                    .query(query)
                    .header("Authorization", auth)
                    .header("x-tidal-client-version", CLIENT_VERSION)
            })
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if status.is_success() { Ok(body) } else { Err(AuthError::Api { status: status.as_u16(), body }) }
    }

    async fn send_get_etag(
        &self,
        url: &str,
        query: &[(&str, &str)],
        auth: &str,
    ) -> Result<Option<String>> {
        let resp = self
            .send_with_rate_limit(|| {
                self.http
                    .get(url)
                    .query(query)
                    .header("Authorization", auth)
                    .header("x-tidal-client-version", CLIENT_VERSION)
            })
            .await?;
        let status = resp.status();
        let etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(ToString::to_string);
        if status.is_success() {
            Ok(etag)
        } else {
            let body = resp.text().await?;
            Err(AuthError::Api { status: status.as_u16(), body })
        }
    }

    async fn send_post_form(
        &self,
        url: &str,
        query: &[(&str, &str)],
        form: &[(&str, &str)],
        if_none_match: Option<&str>,
        auth: &str,
    ) -> Result<String> {
        let resp = self
            .send_with_rate_limit(|| {
                let mut req = self
                    .http
                    .post(url)
                    .query(query)
                    .header("Authorization", auth)
                    .header("x-tidal-client-version", CLIENT_VERSION)
                    .form(form);
                if let Some(etag) = if_none_match {
                    req = req.header("If-None-Match", etag);
                }
                req
            })
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if status.is_success() { Ok(body) } else { Err(AuthError::Api { status: status.as_u16(), body }) }
    }

    async fn send_delete(
        &self,
        url: &str,
        query: &[(&str, &str)],
        if_none_match: Option<&str>,
        auth: &str,
    ) -> Result<String> {
        let resp = self
            .send_with_rate_limit(|| {
                let mut req = self
                    .http
                    .delete(url)
                    .query(query)
                    .header("Authorization", auth)
                    .header("x-tidal-client-version", CLIENT_VERSION);
                if let Some(etag) = if_none_match {
                    req = req.header("If-None-Match", etag);
                }
                req
            })
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if status.is_success() { Ok(body) } else { Err(AuthError::Api { status: status.as_u16(), body }) }
    }

    async fn send_jsonapi_write(
        &self,
        method: reqwest::Method,
        url: &str,
        query: &[(&str, &str)],
        payload: Option<&serde_json::Value>,
        auth: &str,
    ) -> Result<String> {
        let body_str = payload.map(|json| json.to_string());
        let resp = self
            .send_with_rate_limit(|| {
                let mut req = self
                    .http
                    .request(method.clone(), url)
                    .query(query)
                    .header("Authorization", auth)
                    .header("Accept", "application/vnd.api+json");
                if let Some(ref raw) = body_str {
                    req = req
                        .header("Content-Type", "application/vnd.api+json")
                        .body(raw.clone());
                }
                req
            })
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if status.is_success() { Ok(body) } else { Err(AuthError::Api { status: status.as_u16(), body }) }
    }

    /// GET JSON sur `api.tidal.com/v1/{path}` avec un rejeu après refresh sur 401.
    pub async fn get_v1<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let url = format!("{API_V1}/{}", path.trim_start_matches('/'));
        self.get_json_url(&url, query).await
    }

    /// Récupère l'en-tête `ETag` d'une ressource v1 (requis pour les mutations de playlists).
    pub async fn get_v1_etag(&self, path: &str, query: &[(&str, &str)]) -> Result<Option<String>> {
        let url = format!("{API_V1}/{}", path.trim_start_matches('/'));
        let auth = self.authorization().await?;
        match self.send_get_etag(&url, query, &auth).await {
            Err(e) if e.is_unauthorized() => {
                let auth = self.refresh_after_401(&auth).await?;
                self.send_get_etag(&url, query, &auth).await
            }
            other => other,
        }
    }

    /// POST `application/x-www-form-urlencoded` sur `api.tidal.com/v1/{path}` avec rejeu sur 401.
    pub async fn post_v1_form(
        &self,
        path: &str,
        query: &[(&str, &str)],
        form: &[(&str, &str)],
        if_none_match: Option<&str>,
    ) -> Result<String> {
        let url = format!("{API_V1}/{}", path.trim_start_matches('/'));
        let auth = self.authorization().await?;
        match self.send_post_form(&url, query, form, if_none_match, &auth).await {
            Err(e) if e.is_unauthorized() => {
                let auth = self.refresh_after_401(&auth).await?;
                self.send_post_form(&url, query, form, if_none_match, &auth).await
            }
            other => other,
        }
    }

    /// DELETE sur `api.tidal.com/v1/{path}` avec rejeu sur 401.
    pub async fn delete_v1(
        &self,
        path: &str,
        query: &[(&str, &str)],
        if_none_match: Option<&str>,
    ) -> Result<String> {
        let url = format!("{API_V1}/{}", path.trim_start_matches('/'));
        let auth = self.authorization().await?;
        match self.send_delete(&url, query, if_none_match, &auth).await {
            Err(e) if e.is_unauthorized() => {
                let auth = self.refresh_after_401(&auth).await?;
                self.send_delete(&url, query, if_none_match, &auth).await
            }
            other => other,
        }
    }

    /// POST JSON:API sur `https://openapi.tidal.com/v2/{path}` avec rejeu sur 401.
    pub async fn post_openapi_v2(
        &self,
        path: &str,
        query: &[(&str, &str)],
        payload: &serde_json::Value,
    ) -> Result<String> {
        let url = format!("{OPENAPI_V2}/{}", path.trim_start_matches('/'));
        let auth = self.authorization().await?;
        match self
            .send_jsonapi_write(reqwest::Method::POST, &url, query, Some(payload), &auth)
            .await
        {
            Err(e) if e.is_unauthorized() => {
                let auth = self.refresh_after_401(&auth).await?;
                self.send_jsonapi_write(reqwest::Method::POST, &url, query, Some(payload), &auth)
                    .await
            }
            other => other,
        }
    }

    /// PATCH JSON:API sur `https://openapi.tidal.com/v2/{path}` avec rejeu sur 401.
    pub async fn patch_openapi_v2(
        &self,
        path: &str,
        query: &[(&str, &str)],
        payload: &serde_json::Value,
    ) -> Result<String> {
        let url = format!("{OPENAPI_V2}/{}", path.trim_start_matches('/'));
        let auth = self.authorization().await?;
        match self
            .send_jsonapi_write(reqwest::Method::PATCH, &url, query, Some(payload), &auth)
            .await
        {
            Err(e) if e.is_unauthorized() => {
                let auth = self.refresh_after_401(&auth).await?;
                self.send_jsonapi_write(reqwest::Method::PATCH, &url, query, Some(payload), &auth)
                    .await
            }
            other => other,
        }
    }

    /// DELETE JSON:API sur `https://openapi.tidal.com/v2/{path}` avec rejeu sur 401.
    pub async fn delete_openapi_v2(
        &self,
        path: &str,
        query: &[(&str, &str)],
        payload: Option<&serde_json::Value>,
    ) -> Result<String> {
        let url = format!("{OPENAPI_V2}/{}", path.trim_start_matches('/'));
        let auth = self.authorization().await?;
        match self
            .send_jsonapi_write(reqwest::Method::DELETE, &url, query, payload, &auth)
            .await
        {
            Err(e) if e.is_unauthorized() => {
                let auth = self.refresh_after_401(&auth).await?;
                self.send_jsonapi_write(reqwest::Method::DELETE, &url, query, payload, &auth)
                    .await
            }
            other => other,
        }
    }

    /// GET JSON sur `api.tidal.com/v2/{path}` avec un rejeu après refresh sur 401.
    pub async fn get_v2<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let url = format!("https://api.tidal.com/v2/{}", path.trim_start_matches('/'));
        self.get_json_url(&url, query).await
    }

    /// GET JSON:API officiel sur `https://openapi.tidal.com/v2/{path}` avec `Accept: application/vnd.api+json`.
    pub async fn get_openapi_v2<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T> {
        let url = format!("{OPENAPI_V2}/{}", path.trim_start_matches('/'));
        let auth = self.authorization().await?;
        let body = match self.send_get_jsonapi(&url, query, &auth).await {
            Err(e) if e.is_unauthorized() => {
                let auth = self.refresh_after_401(&auth).await?;
                self.send_get_jsonapi(&url, query, &auth).await?
            }
            other => other?,
        };
        Ok(serde_json::from_str(&body)?)
    }

    async fn send_get_jsonapi(&self, url: &str, query: &[(&str, &str)], auth: &str) -> Result<String> {
        let resp = self
            .send_with_rate_limit(|| {
                self.http
                    .get(url)
                    .query(query)
                    .header("Authorization", auth)
                    .header("Accept", "application/vnd.api+json")
            })
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if status.is_success() {
            Ok(body)
        } else {
            Err(AuthError::Api {
                status: status.as_u16(),
                body,
            })
        }
    }

    async fn get_json_url<T: DeserializeOwned>(&self, url: &str, query: &[(&str, &str)]) -> Result<T> {
        let auth = self.authorization().await?;
        let body = match self.send_get(url, query, &auth).await {
            Err(e) if e.is_unauthorized() => {
                let auth = self.refresh_after_401(&auth).await?;
                self.send_get(url, query, &auth).await?
            }
            other => other?,
        };
        Ok(serde_json::from_str(&body)?)
    }

    /// `GET /v1/sessions` (avec repli officiel OpenAPI v2 `https://openapi.tidal.com/v2/users/me`
    /// pour les jetons TIDAL Developer Portal `THIRD_PARTY`).
    pub async fn session(&self) -> Result<Session> {
        let s: Session = match self.get_v1("sessions", &[]).await {
            Ok(sess) => sess,
            Err(_) => self.openapi_users_me().await?,
        };
        let mut guard = self.token.lock().await;
        if guard.user_id != Some(s.user_id) || guard.country_code.as_deref() != Some(&s.country_code) {
            guard.user_id = Some(s.user_id);
            guard.country_code = Some(s.country_code.clone());
            store::save(&guard)?;
        }
        Ok(s)
    }

    /// Repli officiel OpenAPI v2 JSON:API `GET https://openapi.tidal.com/v2/users/me`.
    async fn openapi_users_me(&self) -> Result<Session> {
        let v: serde_json::Value = self.get_openapi_v2("users/me", &[]).await?;
        let user_id = v
            .pointer("/data/id")
            .and_then(|x| x.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let country_code = v
            .pointer("/data/attributes/country")
            .and_then(|x| x.as_str())
            .unwrap_or("FR")
            .to_string();
        Ok(Session {
            session_id: "openapi-v2".into(),
            user_id,
            country_code,
        })
    }

    /// Informations de lecture d'un titre.
    /// Comme `tidalapi` : `playbackinfo` pour un token PKCE,
    /// `playbackinfopostpaywall` pour un token Device Code.
    pub async fn playback_info(&self, track_id: u64, quality: &str) -> Result<PlaybackInfo> {
        let (is_pkce, country) = {
            let t = self.token.lock().await;
            (t.is_pkce, t.country_code.clone().unwrap_or_else(|| "US".into()))
        };
        let endpoint = if is_pkce { "playbackinfo" } else { "playbackinfopostpaywall" };
        self.get_v1(
            &format!("tracks/{track_id}/{endpoint}"),
            &[
                ("audioquality", quality),
                ("playbackmode", "STREAM"),
                ("assetpresentation", "FULL"),
                ("countryCode", &country),
            ],
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::PlaybackInfo;

    fn playback_json(extra: &str) -> String {
        format!(
            r#"{{
                "trackId": 123456,
                "audioQuality": "LOSSLESS",
                "manifestMimeType": "application/dash+xml",
                "manifest": "PHNlZ21lbnRzPjwvc2VnbWVudHM+"
                {extra}
            }}"#
        )
    }

    #[test]
    fn playback_info_parses_track_and_album_replay_gain() {
        let body = playback_json(
            r#",
            "trackReplayGain": -6.5,
            "trackPeakAmplitude": 0.98,
            "albumReplayGain": -4.25,
            "albumPeakAmplitude": 0.91"#,
        );
        let info: PlaybackInfo = serde_json::from_str(&body).expect("playbackinfo TIDAL");
        assert_eq!(info.track_id, 123_456);
        assert_eq!(info.audio_quality, "LOSSLESS");
        assert_eq!(info.track_replay_gain, Some(-6.5));
        assert_eq!(info.track_peak_amplitude, Some(0.98));
        assert_eq!(info.album_replay_gain, Some(-4.25));
        assert_eq!(info.album_peak_amplitude, Some(0.91));
    }

    /// Les réponses sans gain album (titres hors album ou catalogue incomplet)
    /// doivent rester désérialisables : le repli sur la piste est géré par le lecteur.
    #[test]
    fn playback_info_tolerates_missing_replay_gain_fields() {
        let body = playback_json(r#", "trackReplayGain": -3.0"#);
        let info: PlaybackInfo = serde_json::from_str(&body).expect("playbackinfo minimal");
        assert_eq!(info.track_replay_gain, Some(-3.0));
        assert_eq!(info.track_peak_amplitude, None);
        assert_eq!(info.album_replay_gain, None);
        assert_eq!(info.album_peak_amplitude, None);
    }

    #[test]
    fn rate_limit_delay_parses_retry_after_and_caps_bounds() {
        use super::compute_rate_limit_delay;
        use std::time::Duration;

        assert_eq!(compute_rate_limit_delay(Some("2"), 0), Duration::from_secs(2));
        assert_eq!(compute_rate_limit_delay(Some("0"), 0), Duration::from_millis(100));
        assert_eq!(compute_rate_limit_delay(Some("120"), 0), Duration::from_secs(15));
        assert_eq!(compute_rate_limit_delay(Some("1.5"), 0), Duration::from_millis(1500));
        assert_eq!(compute_rate_limit_delay(None, 0), Duration::from_millis(500));
        assert_eq!(compute_rate_limit_delay(Some("invalid"), 1), Duration::from_millis(1500));
        assert_eq!(compute_rate_limit_delay(None, 2), Duration::from_millis(3500));
    }
}
