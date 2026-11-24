//! Identifiants client TIDAL.
//!
//! Comme le `tidal.oxplugin` d'OxiTide, ils sont fournis **hors du binaire** :
//! 1. variables d'environnement `TIDAL_CLIENT_ID`, `TIDAL_CLIENT_SECRET`,
//!    `TIDAL_PKCE_CLIENT_ID`, `TIDAL_PKCE_CLIENT_SECRET` ;
//! 2. sinon `%LOCALAPPDATA%\TideNative\tidal-credentials.json` :
//!
//! ```json
//! { "client_id": "...", "client_secret": "...",
//!   "pkce_client_id": "...", "pkce_client_secret": "..." }
//! ```
//!
//! Ne jamais committer ces valeurs : elles ne sont pas des secrets au sens
//! cryptographique, mais leur redistribution est contraire aux CGU TIDAL.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{AuthError, Result};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ClientCredentials {
    /// Paire « Device Code » (TV / appareil).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Paire « PKCE » (TIDAL Developer Portal THIRD_PARTY ou client Android/Desktop).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pkce_client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pkce_client_secret: Option<String>,
    /// URI de redirection enregistrée sur `developer.tidal.com` (ex. `http://localhost:8989/callback`)
    /// ou `https://tidal.com/android/login/auth` (mode legacy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    /// Scopes OAuth 2.0 personnalisés (par défaut scopes officiels THIRD_PARTY ou legacy selon `redirect_uri`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

impl std::fmt::Debug for ClientCredentials {
    // Ne jamais imprimer les secrets dans les logs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientCredentials")
            .field("client_id", &self.client_id.as_ref().map(|_| "<set>"))
            .field("pkce_client_id", &self.pkce_client_id.as_ref().map(|_| "<set>"))
            .finish_non_exhaustive()
    }
}

/// Paire (id, secret optionnel) sélectionnée pour un flux donné.
#[derive(Clone, Copy)]
pub struct ClientPair<'a> {
    pub id: &'a str,
    pub secret: Option<&'a str>,
}

impl ClientCredentials {
    /// Dossier de données local centralisé (`tidal_core::paths::app_data_dir()`),
    /// basé sur `directories::ProjectDirs::from("", "", "TideNative")` + `create_dir_all` + `dunce`.
    pub fn default_dir() -> Option<PathBuf> {
        tidal_core::paths::app_data_dir().ok()
    }

    pub fn default_path() -> Option<PathBuf> {
        tidal_core::paths::data_file("tidal-credentials.json").ok()
    }

    /// Enregistre les identifiants dans `<data_local_dir>/tidal-credentials.json`
    /// en garantissant l'existence du dossier parent (`create_dir_all`) et en
    /// normalisant le chemin retourné avec `dunce::canonicalize`.
    pub fn save(&self) -> Result<PathBuf> {
        let dir = tidal_core::paths::app_data_dir().map_err(|source| AuthError::CredentialsFile {
            path: "TideNative".into(),
            source,
        })?;
        let path = dir.join("tidal-credentials.json");
        let json = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|source| AuthError::CredentialsFile {
            path: tmp.display().to_string(),
            source,
        })?;
        if let Err(_rename_err) = std::fs::rename(&tmp, &path) {
            let _ = std::fs::remove_file(&tmp);
            std::fs::write(&path, &json).map_err(|source| AuthError::CredentialsFile {
                path: path.display().to_string(),
                source,
            })?;
        }
        Ok(tidal_core::paths::normalize_path(&path))
    }

    /// Charge le fichier (s'il existe) puis applique les variables d'environnement.
    pub fn load() -> Result<Self> {
        let mut creds = match Self::default_path() {
            Some(path) if path.exists() => {
                let raw = std::fs::read_to_string(&path).map_err(|source| {
                    AuthError::CredentialsFile { path: path.display().to_string(), source }
                })?;
                serde_json::from_str::<Self>(&raw)?
            }
            _ => Self::default(),
        };
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        if let Some(v) = env("TIDAL_CLIENT_ID") { creds.client_id = Some(v) }
        if let Some(v) = env("TIDAL_CLIENT_SECRET") { creds.client_secret = Some(v) }
        if let Some(v) = env("TIDAL_PKCE_CLIENT_ID") { creds.pkce_client_id = Some(v) }
        if let Some(v) = env("TIDAL_PKCE_CLIENT_SECRET") { creds.pkce_client_secret = Some(v) }
        if let Some(v) = env("TIDAL_REDIRECT_URI") { creds.redirect_uri = Some(v) }
        if let Some(v) = env("TIDAL_SCOPE") { creds.scope = Some(v) }
        Ok(creds)
    }

    /// Paire à utiliser selon le flux d'origine du token (`is_pkce`).
    /// C'est ce drapeau qui décide aussi de la paire utilisée pour le refresh.
    pub fn pair(&self, is_pkce: bool) -> Result<ClientPair<'_>> {
        fn non_empty(opt: &Option<String>) -> Option<&str> {
            opt.as_deref().map(str::trim).filter(|s| !s.is_empty())
        }
        if is_pkce {
            Ok(ClientPair {
                id: non_empty(&self.pkce_client_id)
                    .or_else(|| non_empty(&self.client_id))
                    .ok_or(AuthError::MissingCredentials("pkce_client_id"))?,
                secret: non_empty(&self.pkce_client_secret)
                    .or_else(|| non_empty(&self.client_secret)),
            })
        } else {
            Ok(ClientPair {
                id: non_empty(&self.client_id)
                    .ok_or(AuthError::MissingCredentials("client_id"))?,
                secret: non_empty(&self.client_secret),
            })
        }
    }
}
