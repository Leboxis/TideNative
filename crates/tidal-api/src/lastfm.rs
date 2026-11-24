//! Client **Last.fm API 2.0** natif (scrobbling) : signature `api_sig`
//! (MD5 RFC 1321 maison, zéro dépendance externe), session mobile et envoi
//! « en cours de lecture » / scrobble.

use crate::{ApiError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tidal_core::TrackSummary;

/// Endpoint unique de l'API Last.fm 2.0 (toutes méthodes en `POST`).
pub const LASTFM_API_URL: &str = "https://ws.audioscrobbler.com/2.0/";

/// Identifiants Last.fm persistés : clé d'API, secret de signature, session
/// utilisateur (`sk`) et nom de compte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastFmCredentials {
    pub api_key: String,
    pub api_secret: String,
    pub session_key: String,
    pub username: String,
}

/// Calcule `api_sig` : paramètres triés alphabétiquement, `clé + valeur`
/// concaténés (`format`, `callback` et `api_sig` exclus), puis le secret
/// d'API, le tout haché en MD5 minuscule (32 caractères hexadécimaux).
pub fn build_api_sig(params: &[(&str, &str)], api_secret: &str) -> String {
    let mut filtered: Vec<(&str, &str)> = params
        .iter()
        .copied()
        .filter(|(key, _)| {
            !key.eq_ignore_ascii_case("format")
                && !key.eq_ignore_ascii_case("callback")
                && !key.eq_ignore_ascii_case("api_sig")
        })
        .collect();
    filtered.sort_by(|a, b| a.0.cmp(b.0));

    let mut raw = String::new();
    for (key, value) in filtered {
        raw.push_str(key);
        raw.push_str(value);
    }
    raw.push_str(api_secret);
    md5_hex(raw.as_bytes())
}

/// Ouvre une session Last.fm (`auth.getMobileSession`) à partir du compte.
pub async fn authenticate_mobile(
    http: &reqwest::Client,
    api_key: &str,
    api_secret: &str,
    username: &str,
    password: &str,
) -> Result<LastFmCredentials> {
    let mut params: Vec<(&str, &str)> = vec![
        ("method", "auth.getMobileSession"),
        ("username", username),
        ("password", password),
        ("api_key", api_key),
    ];
    let signature = build_api_sig(&params, api_secret);
    params.push(("api_sig", &signature));
    params.push(("format", "json"));

    let body = post_form(http, &params).await?;
    parse_auth_session(api_key, api_secret, &body)
}

/// Signale le titre en cours de lecture (`track.updateNowPlaying`).
pub async fn update_now_playing(
    http: &reqwest::Client,
    creds: &LastFmCredentials,
    track: &TrackSummary,
) -> Result<()> {
    let duration = track.duration_secs.to_string();
    let mut params: Vec<(&str, &str)> = vec![
        ("method", "track.updateNowPlaying"),
        ("artist", track.artist_name.as_str()),
        ("track", track.title.as_str()),
        ("api_key", creds.api_key.as_str()),
        ("sk", creds.session_key.as_str()),
    ];
    if !track.album_title.trim().is_empty() {
        params.push(("album", track.album_title.as_str()));
    }
    if track.duration_secs > 0 {
        params.push(("duration", &duration));
    }
    post_signed(http, params, &creds.api_secret).await
}

/// Enregistre une écoute (`track.scrobble`) à l'horodatage Unix fourni.
pub async fn scrobble_track(
    http: &reqwest::Client,
    creds: &LastFmCredentials,
    track: &TrackSummary,
    timestamp_unix: u64,
) -> Result<()> {
    let timestamp = timestamp_unix.to_string();
    let duration = track.duration_secs.to_string();
    let mut params: Vec<(&str, &str)> = vec![
        ("method", "track.scrobble"),
        ("artist", track.artist_name.as_str()),
        ("track", track.title.as_str()),
        ("timestamp", &timestamp),
        ("api_key", creds.api_key.as_str()),
        ("sk", creds.session_key.as_str()),
    ];
    if !track.album_title.trim().is_empty() {
        params.push(("album", track.album_title.as_str()));
    }
    if track.duration_secs > 0 {
        params.push(("duration", &duration));
    }
    post_signed(http, params, &creds.api_secret).await
}

async fn post_signed(
    http: &reqwest::Client,
    mut params: Vec<(&str, &str)>,
    api_secret: &str,
) -> Result<()> {
    let signature = build_api_sig(&params, api_secret);
    params.push(("api_sig", &signature));
    params.push(("format", "json"));

    let body = post_form(http, &params).await?;
    ensure_success(&body)
}

async fn post_form(http: &reqwest::Client, params: &[(&str, &str)]) -> Result<String> {
    let response = http
        .post(LASTFM_API_URL)
        .form(params)
        .send()
        .await
        .map_err(|e| ApiError::Unexpected(format!("Last.fm : requête HTTP échouée : {e}")))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| ApiError::Unexpected(format!("Last.fm : réponse illisible : {e}")))?;
    if !status.is_success() {
        let message = lastfm_error_message(&body)
            .unwrap_or_else(|| format!("Last.fm : HTTP {}", status.as_u16()));
        return Err(ApiError::Unexpected(message));
    }
    Ok(body)
}

/// Parse la réponse `auth.getMobileSession` (succès `session` ou erreur `error`/`message`).
fn parse_auth_session(api_key: &str, api_secret: &str, body: &str) -> Result<LastFmCredentials> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| ApiError::Unexpected(format!("Last.fm : JSON invalide : {e}")))?;
    if let Some(message) = lastfm_error_from_value(&value) {
        return Err(ApiError::Unexpected(message));
    }
    let session = value.get("session").ok_or_else(|| {
        ApiError::Unexpected("Last.fm : réponse auth.getMobileSession sans session".into())
    })?;
    let session_key = session
        .get("key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ApiError::Unexpected("Last.fm : session.key absent".into()))?;
    let username = session
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| ApiError::Unexpected("Last.fm : session.name absent".into()))?;

    Ok(LastFmCredentials {
        api_key: api_key.to_string(),
        api_secret: api_secret.to_string(),
        session_key: session_key.to_string(),
        username: username.to_string(),
    })
}

fn ensure_success(body: &str) -> Result<()> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| ApiError::Unexpected(format!("Last.fm : JSON invalide : {e}")))?;
    match lastfm_error_from_value(&value) {
        Some(message) => Err(ApiError::Unexpected(message)),
        None => Ok(()),
    }
}

fn lastfm_error_message(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    lastfm_error_from_value(&value)
}

fn lastfm_error_from_value(value: &Value) -> Option<String> {
    if !value.is_object() {
        return None;
    }
    let code = value
        .get("error")
        .and_then(|raw| {
            raw.as_i64()
                .or_else(|| raw.as_str().and_then(|text| text.parse::<i64>().ok()))
        });
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty());
    match (code, message) {
        (Some(code), Some(message)) => Some(format!("Last.fm {code} : {message}")),
        (Some(code), None) => Some(format!("Last.fm {code}")),
        (None, Some(message)) => Some(message.to_string()),
        (None, None) => None,
    }
}

/// MD5 RFC 1321 (`d41d8cd98f00b204e9800998ecf8427e` pour l'entrée vide),
/// rendu en 32 caractères hexadécimaux minuscules.
pub fn md5_hex(input: &[u8]) -> String {
    const SHIFTS: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const TABLE: [u32; 64] = [
        0xd76a_a478, 0xe8c7_b756, 0x2420_70db, 0xc1bd_ceee, 0xf57c_0faf, 0x4787_c62a, 0xa830_4613,
        0xfd46_9501, 0x6980_98d8, 0x8b44_f7af, 0xffff_5bb1, 0x895c_d7be, 0x6b90_1122, 0xfd98_7193,
        0xa679_438e, 0x49b4_0821, 0xf61e_2562, 0xc040_b340, 0x265e_5a51, 0xe9b6_c7aa, 0xd62f_105d,
        0x0244_1453, 0xd8a1_e681, 0xe7d3_fbc8, 0x21e1_cde6, 0xc337_07d6, 0xf4d5_0d87, 0x455a_14ed,
        0xa9e3_e905, 0xfcef_a3f8, 0x676f_02d9, 0x8d2a_4c8a, 0xfffa_3942, 0x8771_f681, 0x6d9d_6122,
        0xfde5_380c, 0xa4be_ea44, 0x4bde_cfa9, 0xf6bb_4b60, 0xbebf_bc70, 0x289b_7ec6, 0xeaa1_27fa,
        0xd4ef_3085, 0x0488_1d05, 0xd9d4_d039, 0xe6db_99e5, 0x1fa2_7cf8, 0xc4ac_5665, 0xf429_2244,
        0x432a_ff97, 0xab94_23a7, 0xfc93_a039, 0x655b_59c3, 0x8f0c_cc92, 0xffef_f47d, 0x8584_5dd1,
        0x6fa8_7e4f, 0xfe2c_e6e0, 0xa301_4314, 0x4e08_11a1, 0xf753_7e82, 0xbd3a_f235, 0x2ad7_d2bb,
        0xeb86_d391,
    ];

    let mut state: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut message = Vec::with_capacity(input.len() + 72);
    message.extend_from_slice(input);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_le_bytes());

    for chunk in message.chunks_exact(64) {
        let mut words = [0u32; 16];
        for (index, word) in chunk.chunks_exact(4).enumerate() {
            words[index] = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
        }

        let [mut a, mut b, mut c, mut d] = state;
        for round in 0..64 {
            let (function, index) = match round {
                0..=15 => ((b & c) | (!b & d), round),
                16..=31 => ((d & b) | (!d & c), (5 * round + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * round + 5) % 16),
                _ => (c ^ (b | !d), (7 * round) % 16),
            };
            let sum = a
                .wrapping_add(function)
                .wrapping_add(TABLE[round])
                .wrapping_add(words[index]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(sum.rotate_left(SHIFTS[round]));
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }

    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(32);
    for word in state {
        for byte in word.to_le_bytes() {
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn md5_matches_rfc1321_test_vectors() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"a"), "0cc175b9c0f1b6a831c399e269772661");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(md5_hex(b"message digest"), "f96b697d7cb7938d525a2f31aaf161d0");
        assert_eq!(
            md5_hex(b"abcdefghijklmnopqrstuvwxyz"),
            "c3fcd3d76192e4007dfb496cca67e13b"
        );
        assert_eq!(
            md5_hex(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"),
            "d174ab98d277d9f5a5611c2c9f419d9f"
        );
    }

    #[test]
    fn api_sig_sorts_keys_and_excludes_format_and_callback() {
        let params = [
            ("track", "Digital Love"),
            ("format", "json"),
            ("api_key", "key"),
            ("callback", "cb"),
            ("artist", "Daft Punk"),
        ];
        let signature = build_api_sig(&params, "secret");

        assert_eq!(signature.len(), 32);
        assert!(signature.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        // Ordre alphabétique : api_key + artist + track, puis le secret.
        assert_eq!(
            signature,
            md5_hex(b"api_keykeyartistDaft PunktrackDigital Lovesecret")
        );
        // L'ordre d'entrée et les paramètres exclus ne changent rien.
        let reordered = [
            ("artist", "Daft Punk"),
            ("callback", "autre"),
            ("track", "Digital Love"),
            ("api_key", "key"),
        ];
        assert_eq!(build_api_sig(&reordered, "secret"), signature);
        assert_eq!(build_api_sig(&[], "secret"), md5_hex(b"secret"));
    }

    #[test]
    fn parses_mobile_session_success_payload() {
        let body = json!({
            "session": {
                "name": "Kedi",
                "key": "c0ffee00c0ffee00c0ffee00c0ffee00",
                "subscriber": 1
            }
        })
        .to_string();
        let credentials = parse_auth_session("api-key", "api-secret", &body).unwrap();
        assert_eq!(
            credentials,
            LastFmCredentials {
                api_key: "api-key".into(),
                api_secret: "api-secret".into(),
                session_key: "c0ffee00c0ffee00c0ffee00c0ffee00".into(),
                username: "Kedi".into(),
            }
        );
    }

    #[test]
    fn rejects_mobile_session_error_and_missing_fields() {
        let error = json!({
            "error": 4,
            "message": "Authentication Failed: Invalid username or password"
        })
        .to_string();
        match parse_auth_session("k", "s", &error) {
            Err(ApiError::Unexpected(message)) => {
                assert!(message.contains("Last.fm 4"));
                assert!(message.contains("Authentication Failed"));
            }
            other => panic!("erreur Last.fm attendue, obtenu {other:?}"),
        }

        let missing = json!({ "session": { "name": "Kedi" } }).to_string();
        assert!(matches!(
            parse_auth_session("k", "s", &missing),
            Err(ApiError::Unexpected(_))
        ));
        let empty = json!({ "session": { "key": "  ", "name": "Kedi" } }).to_string();
        assert!(matches!(
            parse_auth_session("k", "s", &empty),
            Err(ApiError::Unexpected(_))
        ));
        assert!(matches!(
            parse_auth_session("k", "s", "pas du json"),
            Err(ApiError::Unexpected(_))
        ));
    }

    #[test]
    fn lastfm_errors_are_detected_on_scrobble_payloads() {
        assert!(ensure_success(r#"{"scrobbles":{"@attr":{"accepted":1}}}"#).is_ok());
        let failure = ensure_success(r#"{"error":9,"message":"Invalid session key"}"#);
        match failure {
            Err(ApiError::Unexpected(message)) => {
                assert_eq!(message, "Last.fm 9 : Invalid session key");
            }
            other => panic!("erreur Last.fm attendue, obtenu {other:?}"),
        }
        assert!(ensure_success(r#"{"nowplaying":{}}"#).is_ok());
        assert!(ensure_success("pas du json").is_err());
    }
}
