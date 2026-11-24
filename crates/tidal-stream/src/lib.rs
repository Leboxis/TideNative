//! Résolution et téléchargement progressif en mémoire des flux audio TIDAL
//! (MPEG-DASH `application/dash+xml` 24-bit/192 kHz et BTS `application/vnd.tidal.bts`).

pub mod buffer;
pub mod downloader;
pub mod mpd;

pub use buffer::{ProgressiveBuffer, ProgressiveReader, RangeRequest};
pub use downloader::{DashTrackCache, StreamDownloader, StreamPlan};
pub use mpd::{DashSegment, DashStreamPlan, parse_mpd};

#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("erreur d'authentification / manifest : {0}")]
    Auth(#[from] tidal_auth::AuthError),

    #[error("manifest MPD MPEG-DASH invalide : {0}")]
    InvalidMpd(String),

    #[error("manifest vide : {0}")]
    EmptyManifest(String),

    #[error("erreur réseau lors du téléchargement du flux : {0}")]
    Http(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, StreamError>;
