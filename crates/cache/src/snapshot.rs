//! Snapshot de démarrage instantané (< 5 ms à froid) écrit de façon atomique
//! (`fichier temporaire` + `rename`) dans le dossier `ProjectDirs` de `TideNative`.

use crate::Result;
use std::path::{Path, PathBuf};
use tidal_core::UiSnapshot;

#[derive(Clone)]
pub struct SnapshotStore {
    path: PathBuf,
}

impl SnapshotStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Charge le snapshot UI depuis le disque ; retourne `UiSnapshot::default()` s'il n'existe pas encore.
    pub fn load_or_default(&self) -> UiSnapshot {
        let Ok(bytes) = std::fs::read(&self.path) else {
            return UiSnapshot::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    /// Sauvegarde atomiquement le snapshot sur disque.
    pub fn save(&self, snapshot: &UiSnapshot) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp_path = self.path.with_extension("json.tmp");
        let payload = serde_json::to_vec(snapshot)?;
        std::fs::write(&tmp_path, payload)?;
        // Sous Windows, rename ne remplace pas toujours un fichier existant avec std::fs::rename
        // si l'OS verrouille brièvement ; on tente rename puis fallback remove+rename.
        if std::fs::rename(&tmp_path, &self.path).is_err() {
            let _ = std::fs::remove_file(&self.path);
            std::fs::rename(&tmp_path, &self.path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidal_core::{AudioQuality, SessionPlaybackState, TrackSummary};

    fn sample_track(id: u64, title: &str) -> TrackSummary {
        TrackSummary {
            id,
            title: title.into(),
            artist_name: "TideNative".into(),
            artist_id: Some(27),
            album_id: Some(100),
            album_title: "Benchmarks".into(),
            cover_id: None,
            duration_secs: 180,
            audio_quality: Some("HI_RES_LOSSLESS".into()),
            explicit: false,
        }
    }

    fn temp_snapshot_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tidenative_snapshot_{tag}_{}.json",
            std::process::id()
        ))
    }

    #[test]
    fn snapshot_atomic_roundtrip() {
        let path = temp_snapshot_path("atomic");
        let _ = std::fs::remove_file(&path);

        let store = SnapshotStore::new(path.clone());
        let mut snap = UiSnapshot::default();
        snap.preferences.exclusive_mode = true;
        snap.preferences.audio_quality = AudioQuality::HiResLossless;
        snap.favorites.push(sample_track(777, "Instant Boot Track"));

        store.save(&snap).unwrap();
        let reloaded = store.load_or_default();
        assert_eq!(reloaded.favorites.len(), 1);
        assert_eq!(reloaded.favorites[0].id, 777);
        assert_eq!(reloaded.favorites[0].artist_id, Some(27));
        assert!(reloaded.preferences.exclusive_mode);

        let _ = std::fs::remove_file(&path);
    }

    /// La file, l'index et la position de reprise doivent survivre à un
    /// aller-retour disque, y compris après une mise à jour du schéma JSON.
    #[test]
    fn snapshot_roundtrips_playback_session() {
        let path = temp_snapshot_path("session");
        let _ = std::fs::remove_file(&path);

        let store = SnapshotStore::new(path.clone());
        let mut snap = UiSnapshot::default();
        snap.playback_session = Some(SessionPlaybackState {
            queue: vec![sample_track(1, "Un"), sample_track(2, "Deux")],
            original_queue: vec![
                sample_track(2, "Deux"),
                sample_track(1, "Un"),
                sample_track(3, "Trois"),
            ],
            queue_index: Some(1),
            position_ms: 42_500,
        });

        store.save(&snap).unwrap();
        let reloaded = store.load_or_default();
        let session = reloaded.playback_session.expect("session persistée");
        assert_eq!(session.queue.len(), 2);
        assert_eq!(session.queue[0].id, 1);
        assert_eq!(session.queue[1].id, 2);
        assert_eq!(session.queue_index, Some(1));
        assert_eq!(session.position_ms, 42_500);
        assert_eq!(session.original_queue.len(), 3);
        assert_eq!(session.original_queue[0].id, 2);

        // Un snapshot historique sans `playback_session` doit se désérialiser
        // normalement (et non retomber sur `default()` à cause d'un champ manquant).
        let mut legacy = UiSnapshot::default();
        legacy.favorites.push(sample_track(5, "Legacy"));
        let mut legacy_json = serde_json::to_value(&legacy).unwrap();
        legacy_json
            .as_object_mut()
            .unwrap()
            .remove("playback_session");
        std::fs::write(&path, serde_json::to_vec(&legacy_json).unwrap()).unwrap();
        let reloaded = store.load_or_default();
        assert!(reloaded.playback_session.is_none());
        assert_eq!(
            reloaded.favorites.first().map(|track| track.id),
            Some(5),
            "le snapshot historique est réellement désérialisé"
        );

        let _ = std::fs::remove_file(&path);
    }
}
