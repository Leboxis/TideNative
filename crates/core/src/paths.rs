//! Résolution centralisée des chemins système Windows/OS pour tout le workspace `TideNative`.
//!
//! Règles strictes :
//! - Résolution unique via `directories::ProjectDirs::from("", "", "TideNative")` (`data_local_dir()`).
//! - Aucune variable d'environnement manuelle (`LOCALAPPDATA`, `USERPROFILE`) ni chemin relatif `./`.
//! - Création garantie du dossier (`std::fs::create_dir_all`) avant toute opération fichier/SQLite.
//! - Normalisation systématique via `dunce::canonicalize` pour éliminer le préfixe verbatim `\\?\`.

use std::io;
use std::path::{Path, PathBuf};

/// Retourne le dossier de données local canonique de `TideNative` après l'avoir créé si besoin.
pub fn app_data_dir() -> io::Result<PathBuf> {
    let proj_dirs = directories::ProjectDirs::from("", "", "TideNative").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Impossible de résoudre le dossier système TideNative via directories::ProjectDirs",
        )
    })?;
    let dir = proj_dirs.data_local_dir().to_path_buf();
    std::fs::create_dir_all(&dir)?;
    Ok(normalize_path(&dir))
}

/// Retourne le chemin d'un fichier dans le dossier de données local après avoir garanti
/// l'existence du dossier parent.
pub fn data_file(filename: &str) -> io::Result<PathBuf> {
    Ok(app_data_dir()?.join(filename))
}

/// Retourne le chemin canonique d'un sous-dossier de `TideNative` après création garantie.
pub fn data_subdir(name: &str) -> io::Result<PathBuf> {
    let sub = app_data_dir()?.join(name);
    std::fs::create_dir_all(&sub)?;
    Ok(normalize_path(&sub))
}

/// Normalise un chemin Windows avec `dunce::canonicalize` afin d'éviter le préfixe `\\?\`.
pub fn normalize_path(path: &Path) -> PathBuf {
    dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_and_normalizes_project_dirs_without_verbatim_prefix() {
        let dir = app_data_dir().expect("ProjectDirs doit résoudre le dossier TideNative");
        assert!(dir.is_dir(), "le dossier doit exister après app_data_dir()");
        let as_str = dir.to_string_lossy();
        assert!(
            !as_str.starts_with(r"\\?\"),
            "le chemin normalisé ne doit pas contenir de préfixe verbatim : {as_str}"
        );
        assert!(
            as_str.to_lowercase().contains("tidenative"),
            "le chemin doit contenir TideNative : {as_str}"
        );
    }
}
