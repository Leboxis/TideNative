//! Décodage du champ `manifest` (base64) renvoyé par `playbackinfo`.
//!
//! - `application/vnd.tidal.bts` : JSON avec la/les URL(s) directes.
//! - `application/dash+xml`     : MPD MPEG-DASH (Hi-Res, FLAC en fMP4).
//!   Le parsing complet (SegmentTemplate/SegmentTimeline) est fait par le
//!   crate `tidal-stream` via `dash-mpd` ; ici on expose le XML brut.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Deserialize;

use crate::error::{AuthError, Result};

pub const MIME_BTS: &str = "application/vnd.tidal.bts";
pub const MIME_DASH: &str = "application/dash+xml";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BtsManifest {
    pub mime_type: String,
    #[serde(default)]
    pub codecs: Option<String>,
    #[serde(default)]
    pub encryption_type: Option<String>,
    pub urls: Vec<String>,
}

#[derive(Debug)]
pub enum Manifest {
    Bts(BtsManifest),
    Dash { mpd_xml: String },
}

impl Manifest {
    pub fn decode(mime_type: &str, manifest_b64: &str) -> Result<Self> {
        let raw = STANDARD.decode(manifest_b64.trim())?;
        match mime_type {
            MIME_BTS => Ok(Manifest::Bts(serde_json::from_slice(&raw)?)),
            MIME_DASH => Ok(Manifest::Dash {
                mpd_xml: String::from_utf8(raw).map_err(|e| AuthError::Manifest(e.to_string()))?,
            }),
            other => Err(AuthError::Manifest(format!("type de manifest inconnu : {other}"))),
        }
    }

    /// Flux chiffré ? (non lisible sans DRM — hors périmètre).
    pub fn is_encrypted(&self) -> bool {
        match self {
            Manifest::Bts(m) => m.encryption_type.as_deref().is_some_and(|e| e != "NONE"),
            Manifest::Dash { mpd_xml } => mpd_xml.contains("ContentProtection"),
        }
    }

    /// Valeur de l'attribut `codecs` (ex. `flac`, `mp4a.40.2`).
    pub fn codecs(&self) -> Option<String> {
        match self {
            Manifest::Bts(m) => m.codecs.clone(),
            Manifest::Dash { mpd_xml } => {
                let start = mpd_xml.find("codecs=\"")? + 8;
                let end = mpd_xml[start..].find('"')?;
                Some(mpd_xml[start..start + end].to_owned())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_bts() {
        let json = r#"{"mimeType":"audio/flac","codecs":"flac","encryptionType":"NONE","urls":["https://x/y.flac"]}"#;
        let b64 = STANDARD.encode(json);
        let m = Manifest::decode(MIME_BTS, &b64).unwrap();
        assert!(!m.is_encrypted());
        assert_eq!(m.codecs().as_deref(), Some("flac"));
    }

    #[test]
    fn decodes_dash_codecs() {
        let xml = r#"<MPD><Period><AdaptationSet><Representation codecs="flac" bandwidth="1"/></AdaptationSet></Period></MPD>"#;
        let m = Manifest::decode(MIME_DASH, &STANDARD.encode(xml)).unwrap();
        assert_eq!(m.codecs().as_deref(), Some("flac"));
    }
}
