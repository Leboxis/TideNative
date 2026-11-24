//! `tidal-auth` — authentification TIDAL (PKCE + Device Code + refresh) et
//! premier appel de lecture (`playbackinfo`), pour le client Windows natif.
//!
//! Modèle repris d'OxiTide (cf. `audit-login-tidal-oxitide.md`) :
//! - les identifiants client ne sont **pas** dans le code : ils sont fournis
//!   à l'exécution (équivalent du `tidal.oxplugin`), voir [`credentials`] ;
//! - le flux principal est **Authorization Code + PKCE (S256)**, seul flux
//!   donnant accès au FLAC / Hi-Res ; le Device Code reste un secours ;
//! - le token est persisté avec un drapeau `is_pkce` qui pilote la paire
//!   d'identifiants utilisée pour les renouvellements et l'endpoint de lecture.
//!
//! Différence volontaire avec OxiTide : le token est stocké dans le
//! Gestionnaire d'identification Windows plutôt que dans un fichier JSON.

pub mod client;
pub mod credentials;
pub mod device;
pub mod error;
pub mod manifest;
pub mod pkce;
pub mod store;
pub mod token;
pub mod win;

pub use client::{PlaybackInfo, Session, TidalClient};
pub use credentials::ClientCredentials;
pub use error::{AuthError, Result};
pub use store::{delete_named_secret, load_named_secret, save_named_secret};
pub use token::TokenSet;

/// Portée demandée par tous les clients tiers connus (tidalapi, OxiTide).
pub const SCOPE: &str = "r_usr w_usr w_sub";
/// Base des endpoints OAuth2 TIDAL.
pub const AUTH_BASE: &str = "https://auth.tidal.com/v1/oauth2";
/// Base de l'API catalogue v1.
pub const API_V1: &str = "https://api.tidal.com/v1";
/// Base de l'API OpenAPI officielle v2 (`THIRD_PARTY`, JSON:API).
pub const OPENAPI_V2: &str = "https://openapi.tidal.com/v2";
/// Version client annoncée (valeur observée dans OxiTide 0.2.3).
pub const CLIENT_VERSION: &str = "2025.7.16";

/// Client HTTP partagé : TLS natif Windows (SChannel), HTTP/2, gzip, TCP_NODELAY.
pub fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("TideNative/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .pool_max_idle_per_host(8)
        .tcp_nodelay(true)
        .build()?)
}
