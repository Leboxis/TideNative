//! Cache local `TideNative` : SQLite WAL + FTS5 (via `winsqlite3.dll` natif Windows),
//! snapshot de démarrage instantané (< 5 ms) et cache pochettes borné en RAM/disque.

pub mod covers;
pub mod db;
pub mod palette;
pub mod snapshot;
#[cfg(windows)]
pub mod winsqlite;

pub use covers::CoverCache;
pub use db::CacheDb;
pub use snapshot::SnapshotStore;

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("erreur SQLite : {0}")]
    Sqlite(String),

    #[error("erreur I/O cache : {0}")]
    Io(#[from] std::io::Error),

    #[error("erreur sérialisation snapshot : {0}")]
    Json(#[from] serde_json::Error),

    #[error("erreur réseau pochette : {0}")]
    Http(#[from] reqwest::Error),

    #[error("identifiant de pochette invalide : {0}")]
    InvalidCoverId(String),

    #[error("statut HTTP inattendu pour la pochette : {0}")]
    HttpStatus(u16),
}

pub type Result<T> = std::result::Result<T, CacheError>;

/// Dossier de données local centralisé (`tidal_core::paths::app_data_dir()`),
/// basé sur `directories::ProjectDirs::from("", "", "TideNative")`,
/// avec `std::fs::create_dir_all` garanti et normalisation `dunce::canonicalize`.
pub fn app_data_dir() -> PathBuf {
    tidal_core::paths::app_data_dir()
        .expect("Impossible de résoudre le dossier de données système TideNative via ProjectDirs")
}

pub fn default_db_path() -> PathBuf {
    app_data_dir().join("library.sqlite3")
}

pub fn default_snapshot_path() -> PathBuf {
    app_data_dir().join("ui-snapshot.json")
}

pub fn default_covers_dir() -> PathBuf {
    tidal_core::paths::data_subdir("covers").unwrap_or_else(|_| app_data_dir().join("covers"))
}
