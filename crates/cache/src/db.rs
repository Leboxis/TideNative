//! Base SQLite locale haute performance (`journal_mode=WAL`, `STRICT` tables,
//! index plein texte **FTS5** sans scan `LIKE` pour recherche instantanée < 1 ms),
//! adossée directement au moteur système Windows `winsqlite3.dll`.

use crate::Result;
use crate::winsqlite::{Connection, Statement};
use std::path::Path;
use tidal_core::{
    ActivityArtistStat, ActivityTrackStat, ListeningActivityStats, PlaylistSummary, TrackSummary,
};

pub struct CacheDb {
    conn: Connection,
}

impl CacheDb {
    /// Ouvre (ou crée) la base SQLite en mode WAL avec les PRAGMA optimaux.
    pub fn open(path: &Path) -> Result<Self> {
        let open_path = if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            let clean_parent = tidal_core::paths::normalize_path(parent);
            match path.file_name() {
                Some(name) => clean_parent.join(name),
                None => path.to_path_buf(),
            }
        } else {
            path.to_path_buf()
        };
        let path_str = open_path.to_string_lossy();
        let conn = Connection::open_str(&path_str)?;
        let db = Self { conn };
        db.configure_pragmas()?;
        db.migrate()?;
        Ok(db)
    }

    /// Ouvre une base SQLite en mémoire (tests unitaires).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_str(":memory:")?;
        let db = Self { conn };
        db.configure_pragmas()?;
        db.migrate()?;
        Ok(db)
    }

    fn configure_pragmas(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA busy_timeout = 5000;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            PRAGMA temp_store = MEMORY;
            PRAGMA cache_size = -16000;
            ",
        )?;
        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS tracks (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                artist_name TEXT NOT NULL,
                artist_id INTEGER NOT NULL DEFAULT 0,
                album_id INTEGER NOT NULL DEFAULT 0,
                album_title TEXT NOT NULL,
                cover_id TEXT,
                duration_secs INTEGER NOT NULL,
                audio_quality TEXT,
                explicit INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT (unixepoch())
            ) STRICT;

            CREATE VIRTUAL TABLE IF NOT EXISTS tracks_fts USING fts5(
                title,
                artist_name,
                album_title,
                content='tracks',
                content_rowid='id',
                tokenize='unicode61 remove_diacritics 2'
            );

            CREATE TRIGGER IF NOT EXISTS tracks_ai AFTER INSERT ON tracks BEGIN
                INSERT INTO tracks_fts(rowid, title, artist_name, album_title)
                VALUES (new.id, new.title, new.artist_name, new.album_title);
            END;

            CREATE TRIGGER IF NOT EXISTS tracks_ad AFTER DELETE ON tracks BEGIN
                INSERT INTO tracks_fts(tracks_fts, rowid, title, artist_name, album_title)
                VALUES ('delete', old.id, old.title, old.artist_name, old.album_title);
            END;

            CREATE TRIGGER IF NOT EXISTS tracks_au AFTER UPDATE ON tracks BEGIN
                INSERT INTO tracks_fts(tracks_fts, rowid, title, artist_name, album_title)
                VALUES ('delete', old.id, old.title, old.artist_name, old.album_title);
                INSERT INTO tracks_fts(rowid, title, artist_name, album_title)
                VALUES (new.id, new.title, new.artist_name, new.album_title);
            END;

            CREATE TABLE IF NOT EXISTS favorites (
                position INTEGER PRIMARY KEY,
                track_id INTEGER NOT NULL UNIQUE REFERENCES tracks(id) ON DELETE CASCADE
            ) STRICT;

            CREATE TABLE IF NOT EXISTS playlists (
                uuid TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT,
                creator TEXT NOT NULL,
                number_of_tracks INTEGER NOT NULL,
                image_id TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0
            ) STRICT;

            CREATE TABLE IF NOT EXISTS playlist_tracks (
                playlist_uuid TEXT NOT NULL REFERENCES playlists(uuid) ON DELETE CASCADE,
                position INTEGER NOT NULL,
                track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                PRIMARY KEY (playlist_uuid, position)
            ) STRICT;

            CREATE INDEX IF NOT EXISTS idx_playlist_tracks_track ON playlist_tracks(track_id);

            CREATE TABLE IF NOT EXISTS listening_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                track_id INTEGER NOT NULL,
                title TEXT NOT NULL,
                artist_name TEXT NOT NULL,
                artist_id INTEGER NOT NULL DEFAULT 0,
                album_id INTEGER NOT NULL DEFAULT 0,
                album_title TEXT NOT NULL,
                cover_id TEXT,
                duration_secs INTEGER NOT NULL,
                played_secs INTEGER NOT NULL,
                played_at INTEGER NOT NULL DEFAULT (unixepoch())
            ) STRICT;

            CREATE INDEX IF NOT EXISTS idx_listening_events_played_at ON listening_events(played_at DESC);
            ",
        )?;
        // Migrations douces pour les bases existantes (idempotentes, sans perte de données)
        let _ = self
            .conn
            .execute_batch("ALTER TABLE tracks ADD COLUMN album_id INTEGER NOT NULL DEFAULT 0;");
        let _ = self
            .conn
            .execute_batch("ALTER TABLE tracks ADD COLUMN artist_id INTEGER NOT NULL DEFAULT 0;");
        Ok(())
    }

    /// Retourne le mode journal actif (`wal` sur disque, `memory` en RAM).
    pub fn journal_mode(&self) -> Result<String> {
        let mut stmt = self.conn.prepare("PRAGMA journal_mode;")?;
        if stmt.step_row()? {
            Ok(stmt.col_str(0))
        } else {
            Ok(String::new())
        }
    }

    /// Retourne le délai `PRAGMA busy_timeout` configuré en millisecondes.
    pub fn busy_timeout_ms(&self) -> Result<i64> {
        let mut stmt = self.conn.prepare("PRAGMA busy_timeout;")?;
        if stmt.step_row()? {
            Ok(stmt.col_i64(0))
        } else {
            Ok(0)
        }
    }

    /// Insère ou met à jour en masse une liste de pistes dans une transaction unique.
    pub fn upsert_tracks(&mut self, tracks: &[TrackSummary]) -> Result<()> {
        self.conn.with_transaction(|conn| {
            upsert_tracks_in_tx(conn, tracks)
        })
    }

    /// Remplace la liste des favoris tout en mettant à jour les métadonnées des pistes.
    pub fn replace_favorites(&mut self, tracks: &[TrackSummary]) -> Result<()> {
        self.conn.with_transaction(|conn| {
            upsert_tracks_in_tx(conn, tracks)?;
            conn.execute_batch("DELETE FROM favorites;")?;
            let mut stmt = conn.prepare(
                "INSERT OR IGNORE INTO favorites (position, track_id) VALUES (?1, ?2);",
            )?;
            for (idx, t) in tracks.iter().enumerate() {
                stmt.bind_i64(1, idx as i64)?;
                stmt.bind_i64(2, t.id as i64)?;
                stmt.execute_and_reset()?;
            }
            Ok(())
        })
    }

    /// Lit tous les favoris dans leur ordre de sauvegarde.
    pub fn load_favorites(&self) -> Result<Vec<TrackSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.title, t.artist_name, t.artist_id, t.album_id, t.album_title, t.cover_id,
                    t.duration_secs, t.audio_quality, t.explicit
             FROM favorites f
             JOIN tracks t ON t.id = f.track_id
             ORDER BY f.position ASC;",
        )?;
        let mut out = Vec::new();
        while stmt.step_row()? {
            out.push(read_track_row(&stmt));
        }
        Ok(out)
    }

    /// Remplace les playlists mises en cache.
    pub fn replace_playlists(&mut self, playlists: &[PlaylistSummary]) -> Result<()> {
        self.conn.with_transaction(|conn| {
            conn.execute_batch("DELETE FROM playlists;")?;
            let mut stmt = conn.prepare(
                "INSERT INTO playlists (uuid, title, description, creator, number_of_tracks, image_id, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7);",
            )?;
            for (idx, p) in playlists.iter().enumerate() {
                stmt.bind_str(1, &p.uuid)?;
                stmt.bind_str(2, &p.title)?;
                stmt.bind_opt_str(3, p.description.as_deref())?;
                stmt.bind_str(4, &p.creator)?;
                stmt.bind_i64(5, i64::from(p.number_of_tracks))?;
                stmt.bind_opt_str(6, p.image_id.as_deref())?;
                stmt.bind_i64(7, idx as i64)?;
                stmt.execute_and_reset()?;
            }
            Ok(())
        })
    }

    pub fn load_playlists(&self) -> Result<Vec<PlaylistSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT uuid, title, description, creator, number_of_tracks, image_id
             FROM playlists
             ORDER BY sort_order ASC;",
        )?;
        let mut out = Vec::new();
        while stmt.step_row()? {
            out.push(PlaylistSummary {
                uuid: stmt.col_str(0),
                title: stmt.col_str(1),
                description: stmt.col_opt_str(2),
                creator: stmt.col_str(3),
                number_of_tracks: stmt.col_i64(4) as u32,
                image_id: stmt.col_opt_str(5),
            });
        }
        Ok(out)
    }

    /// Remplace les pistes associées à une playlist en cache.
    pub fn replace_playlist_tracks(
        &mut self,
        playlist_uuid: &str,
        tracks: &[TrackSummary],
    ) -> Result<()> {
        self.conn.with_transaction(|conn| {
            upsert_tracks_in_tx(conn, tracks)?;
            let mut del = conn.prepare("DELETE FROM playlist_tracks WHERE playlist_uuid = ?1;")?;
            del.bind_str(1, playlist_uuid)?;
            del.execute_and_reset()?;

            let mut ins = conn.prepare(
                "INSERT INTO playlist_tracks (playlist_uuid, position, track_id) VALUES (?1, ?2, ?3);",
            )?;
            for (idx, t) in tracks.iter().enumerate() {
                ins.bind_str(1, playlist_uuid)?;
                ins.bind_i64(2, idx as i64)?;
                ins.bind_i64(3, t.id as i64)?;
                ins.execute_and_reset()?;
            }
            Ok(())
        })
    }

    pub fn load_playlist_tracks(&self, playlist_uuid: &str) -> Result<Vec<TrackSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.title, t.artist_name, t.artist_id, t.album_id, t.album_title, t.cover_id,
                    t.duration_secs, t.audio_quality, t.explicit
             FROM playlist_tracks pt
             JOIN tracks t ON t.id = pt.track_id
             WHERE pt.playlist_uuid = ?1
             ORDER BY pt.position ASC;",
        )?;
        stmt.bind_str(1, playlist_uuid)?;
        let mut out = Vec::new();
        while stmt.step_row()? {
            out.push(read_track_row(&stmt));
        }
        Ok(out)
    }

    /// Recherche instantanée FTS5 (BM25) sur toutes les pistes en cache local.
    pub fn search_local_tracks(&self, query: &str, limit: usize) -> Result<Vec<TrackSummary>> {
        let fts_query = build_fts5_prefix_query(query);
        if fts_query.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.title, t.artist_name, t.artist_id, t.album_id, t.album_title, t.cover_id,
                    t.duration_secs, t.audio_quality, t.explicit
             FROM tracks_fts
             JOIN tracks t ON t.id = tracks_fts.rowid
             WHERE tracks_fts MATCH ?1
             ORDER BY bm25(tracks_fts)
             LIMIT ?2;",
        )?;
        stmt.bind_str(1, &fts_query)?;
        stmt.bind_i64(2, limit as i64)?;
        let mut out = Vec::new();
        while stmt.step_row()? {
            out.push(read_track_row(&stmt));
        }
        Ok(out)
    }

    /// Récupère une piste par identifiant.
    pub fn get_track(&self, track_id: u64) -> Result<Option<TrackSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, artist_name, artist_id, album_id, album_title, cover_id, duration_secs, audio_quality, explicit
             FROM tracks WHERE id = ?1;",
        )?;
        stmt.bind_i64(1, track_id as i64)?;
        if stmt.step_row()? {
            Ok(Some(read_track_row(&stmt)))
        } else {
            Ok(None)
        }
    }

    /// Enregistre une écoute locale horodatée à l'instant présent.
    pub fn record_play_event(&self, track: &TrackSummary, played_secs: u32) -> Result<()> {
        self.record_play_event_at(track, played_secs, unix_now())
    }

    /// Enregistre une écoute locale à un horodatage Unix explicite
    /// (indispensable pour tester les fenêtres glissantes de 30 jours).
    pub fn record_play_event_at(
        &self,
        track: &TrackSummary,
        played_secs: u32,
        played_at: i64,
    ) -> Result<()> {
        self.insert_play_event_at(track, played_secs, played_at)
            .map(|_| ())
    }

    /// Insère un événement d'écoute et retourne son identifiant `id` SQLite
    /// afin de pouvoir mettre à jour `played_secs` au fil de l'écoute réelle.
    pub fn insert_play_event_at(
        &self,
        track: &TrackSummary,
        played_secs: u32,
        played_at: i64,
    ) -> Result<i64> {
        let mut stmt = self.conn.prepare(
            "INSERT INTO listening_events
                (track_id, title, artist_name, artist_id, album_id, album_title, cover_id,
                 duration_secs, played_secs, played_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10);",
        )?;
        stmt.bind_i64(1, track.id as i64)?;
        stmt.bind_str(2, &track.title)?;
        stmt.bind_str(3, &track.artist_name)?;
        stmt.bind_i64(4, track.artist_id.unwrap_or(0) as i64)?;
        stmt.bind_i64(5, track.album_id.unwrap_or(0) as i64)?;
        stmt.bind_str(6, &track.album_title)?;
        stmt.bind_opt_str(7, track.cover_id.as_deref())?;
        stmt.bind_i64(8, i64::from(track.duration_secs))?;
        stmt.bind_i64(9, i64::from(played_secs))?;
        stmt.bind_i64(10, played_at)?;
        stmt.execute_and_reset()?;

        let mut id_stmt = self.conn.prepare("SELECT last_insert_rowid();")?;
        if id_stmt.step_row()? {
            Ok(id_stmt.col_i64(0))
        } else {
            Ok(0)
        }
    }

    /// Met à jour la durée réellement écoutée (`played_secs`) d'un événement actif.
    pub fn update_play_event_duration(&self, event_id: i64, played_secs: u32) -> Result<()> {
        let mut stmt = self
            .conn
            .prepare("UPDATE listening_events SET played_secs = ?1 WHERE id = ?2;")?;
        stmt.bind_i64(1, i64::from(played_secs))?;
        stmt.bind_i64(2, event_id)?;
        stmt.execute_and_reset()
    }

    /// Calcule les statistiques « Mon Activité » : totaux all-time et 30 jours,
    /// artistes uniques, top titres / artistes et écoutes récentes.
    ///
    /// `days` filtre les agrégats (top listes, artistes uniques) sur une fenêtre
    /// glissante ; `total_plays` / `total_secs` et les compteurs 30 jours restent
    /// toujours exacts, quelle que soit la fenêtre demandée.
    pub fn load_listening_stats(
        &self,
        days: Option<u32>,
        limit: usize,
    ) -> Result<ListeningActivityStats> {
        let now = unix_now();
        let window_start = days.map(|d| now - i64::from(d) * SECS_PER_DAY);
        let last_30_start = now - 30 * SECS_PER_DAY;
        let limit = limit.min(i64::MAX as usize) as i64;

        let (total_plays, total_secs) = self.listening_totals(None)?;
        let (plays_last_30_days, secs_last_30_days) = self.listening_totals(Some(last_30_start))?;
        let unique_artists = self.unique_artist_count(window_start)?;

        let (filter, limit_parameter) = match window_start {
            Some(_) => (" WHERE played_at >= ?1", 2),
            None => ("", 1),
        };

        let mut top_tracks = Vec::new();
        {
            let sql = format!(
                "SELECT track_id, MAX(title), MAX(artist_name), MAX(artist_id), MAX(album_id),
                        MAX(album_title), MAX(cover_id), MAX(duration_secs), COUNT(*),
                        COALESCE(SUM(played_secs), 0)
                 FROM listening_events{filter}
                 GROUP BY track_id
                 ORDER BY COUNT(*) DESC, SUM(played_secs) DESC, track_id ASC
                 LIMIT ?{limit_parameter};"
            );
            let mut stmt = self.conn.prepare(&sql)?;
            if let Some(start) = window_start {
                stmt.bind_i64(1, start)?;
            }
            stmt.bind_i64(limit_parameter, limit)?;
            while stmt.step_row()? {
                let raw_artist_id = stmt.col_i64(3);
                let raw_album_id = stmt.col_i64(4);
                top_tracks.push(ActivityTrackStat {
                    track: TrackSummary {
                        id: stmt.col_i64(0).max(0) as u64,
                        title: stmt.col_str(1),
                        artist_name: stmt.col_str(2),
                        artist_id: (raw_artist_id > 0).then_some(raw_artist_id as u64),
                        album_id: (raw_album_id > 0).then_some(raw_album_id as u64),
                        album_title: stmt.col_str(5),
                        cover_id: stmt.col_opt_str(6),
                        duration_secs: stmt.col_i64(7).max(0) as u32,
                        audio_quality: None,
                        explicit: false,
                    },
                    play_count: stmt.col_i64(8).max(0) as u32,
                    total_secs: stmt.col_i64(9).max(0) as u64,
                });
            }
        }

        let mut top_artists = Vec::new();
        {
            let sql = format!(
                "SELECT MIN(artist_name), MAX(artist_id), COUNT(*), COALESCE(SUM(played_secs), 0)
                 FROM listening_events{filter}
                 GROUP BY LOWER(artist_name)
                 ORDER BY COUNT(*) DESC, SUM(played_secs) DESC, LOWER(artist_name) ASC
                 LIMIT ?{limit_parameter};"
            );
            let mut stmt = self.conn.prepare(&sql)?;
            if let Some(start) = window_start {
                stmt.bind_i64(1, start)?;
            }
            stmt.bind_i64(limit_parameter, limit)?;
            while stmt.step_row()? {
                let raw_artist_id = stmt.col_i64(1);
                top_artists.push(ActivityArtistStat {
                    artist_name: stmt.col_str(0),
                    artist_id: (raw_artist_id > 0).then_some(raw_artist_id as u64),
                    play_count: stmt.col_i64(2).max(0) as u32,
                    total_secs: stmt.col_i64(3).max(0) as u64,
                });
            }
        }

        let mut recent_plays = Vec::new();
        {
            let sql = format!(
                "SELECT track_id, title, artist_name, artist_id, album_id, album_title, cover_id,
                        duration_secs
                 FROM listening_events{filter}
                 ORDER BY played_at DESC, id DESC
                 LIMIT ?{limit_parameter};"
            );
            let mut stmt = self.conn.prepare(&sql)?;
            if let Some(start) = window_start {
                stmt.bind_i64(1, start)?;
            }
            stmt.bind_i64(limit_parameter, limit)?;
            while stmt.step_row()? {
                let raw_artist_id = stmt.col_i64(3);
                let raw_album_id = stmt.col_i64(4);
                recent_plays.push(TrackSummary {
                    id: stmt.col_i64(0).max(0) as u64,
                    title: stmt.col_str(1),
                    artist_name: stmt.col_str(2),
                    artist_id: (raw_artist_id > 0).then_some(raw_artist_id as u64),
                    album_id: (raw_album_id > 0).then_some(raw_album_id as u64),
                    album_title: stmt.col_str(5),
                    cover_id: stmt.col_opt_str(6),
                    duration_secs: stmt.col_i64(7).max(0) as u32,
                    audio_quality: None,
                    explicit: false,
                });
            }
        }

        Ok(ListeningActivityStats {
            total_plays,
            total_secs,
            plays_last_30_days,
            secs_last_30_days,
            unique_artists,
            top_tracks,
            top_artists,
            recent_plays,
        })
    }

    /// Efface tout l'historique d'écoute local.
    pub fn clear_listening_history(&self) -> Result<()> {
        self.conn.execute_batch("DELETE FROM listening_events;")
    }

    fn listening_totals(&self, since: Option<i64>) -> Result<(u64, u64)> {
        let sql = if since.is_some() {
            "SELECT COUNT(*), COALESCE(SUM(played_secs), 0) FROM listening_events WHERE played_at >= ?1;"
        } else {
            "SELECT COUNT(*), COALESCE(SUM(played_secs), 0) FROM listening_events;"
        };
        let mut stmt = self.conn.prepare(sql)?;
        if let Some(start) = since {
            stmt.bind_i64(1, start)?;
        }
        if stmt.step_row()? {
            Ok((stmt.col_i64(0).max(0) as u64, stmt.col_i64(1).max(0) as u64))
        } else {
            Ok((0, 0))
        }
    }

    fn unique_artist_count(&self, since: Option<i64>) -> Result<u64> {
        let sql = if since.is_some() {
            "SELECT COUNT(DISTINCT LOWER(artist_name)) FROM listening_events WHERE played_at >= ?1;"
        } else {
            "SELECT COUNT(DISTINCT LOWER(artist_name)) FROM listening_events;"
        };
        let mut stmt = self.conn.prepare(sql)?;
        if let Some(start) = since {
            stmt.bind_i64(1, start)?;
        }
        if stmt.step_row()? {
            Ok(stmt.col_i64(0).max(0) as u64)
        } else {
            Ok(0)
        }
    }
}

/// Nombre de secondes d'une journée, pour les fenêtres glissantes.
const SECS_PER_DAY: i64 = 86_400;

/// Horodatage Unix courant (secondes).
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

fn upsert_tracks_in_tx(conn: &Connection, tracks: &[TrackSummary]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO tracks (id, title, artist_name, artist_id, album_id, album_title, cover_id, duration_secs, audio_quality, explicit, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, unixepoch())
         ON CONFLICT(id) DO UPDATE SET
            title = excluded.title,
            artist_name = excluded.artist_name,
            artist_id = excluded.artist_id,
            album_id = excluded.album_id,
            album_title = excluded.album_title,
            cover_id = excluded.cover_id,
            duration_secs = excluded.duration_secs,
            audio_quality = excluded.audio_quality,
            explicit = excluded.explicit,
            updated_at = excluded.updated_at;",
    )?;
    for t in tracks {
        stmt.bind_i64(1, t.id as i64)?;
        stmt.bind_str(2, &t.title)?;
        stmt.bind_str(3, &t.artist_name)?;
        stmt.bind_i64(4, t.artist_id.unwrap_or(0) as i64)?;
        stmt.bind_i64(5, t.album_id.unwrap_or(0) as i64)?;
        stmt.bind_str(6, &t.album_title)?;
        stmt.bind_opt_str(7, t.cover_id.as_deref())?;
        stmt.bind_i64(8, i64::from(t.duration_secs))?;
        stmt.bind_opt_str(9, t.audio_quality.as_deref())?;
        stmt.bind_i64(10, i64::from(t.explicit))?;
        stmt.execute_and_reset()?;
    }
    Ok(())
}

fn read_track_row(stmt: &Statement<'_>) -> TrackSummary {
    let raw_artist_id = stmt.col_i64(3);
    let raw_album_id = stmt.col_i64(4);
    TrackSummary {
        id: stmt.col_i64(0) as u64,
        title: stmt.col_str(1),
        artist_name: stmt.col_str(2),
        artist_id: (raw_artist_id > 0).then_some(raw_artist_id as u64),
        album_id: (raw_album_id > 0).then_some(raw_album_id as u64),
        album_title: stmt.col_str(5),
        cover_id: stmt.col_opt_str(6),
        duration_secs: stmt.col_i64(7) as u32,
        audio_quality: stmt.col_opt_str(8),
        explicit: stmt.col_i64(9) != 0,
    }
}

/// Transforme une requête utilisateur libre en requête préfixe FTS5 sûre (`"daft"* "punk"*`).
fn build_fts5_prefix_query(raw: &str) -> String {
    raw.split_whitespace()
        .filter_map(|token| {
            let cleaned: String = token
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '\'' || *c == '-')
                .collect();
            if cleaned.is_empty() {
                None
            } else {
                Some(format!("\"{cleaned}\"*"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_track(id: u64, title: &str, artist: &str, album: &str) -> TrackSummary {
        TrackSummary {
            id,
            title: title.into(),
            artist_name: artist.into(),
            artist_id: Some(id * 3),
            album_id: Some(id * 10),
            album_title: album.into(),
            cover_id: Some("a1b2c3d4-0000-0000-0000-000000000000".into()),
            duration_secs: 240,
            audio_quality: Some("HI_RES_LOSSLESS".into()),
            explicit: false,
        }
    }

    #[test]
    fn legacy_schema_without_artist_id_is_migrated_without_data_loss() {
        // Base créée par une version antérieure : `tracks` n'a pas encore `artist_id`,
        // mais le cache FTS5 et ses déclencheurs sont déjà en place.
        let conn = Connection::open_str(":memory:").unwrap();
        conn.execute_batch(
            "
            CREATE TABLE tracks (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                artist_name TEXT NOT NULL,
                album_id INTEGER NOT NULL DEFAULT 0,
                album_title TEXT NOT NULL,
                cover_id TEXT,
                duration_secs INTEGER NOT NULL,
                audio_quality TEXT,
                explicit INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT (unixepoch())
            ) STRICT;

            CREATE VIRTUAL TABLE tracks_fts USING fts5(
                title, artist_name, album_title,
                content='tracks',
                content_rowid='id',
                tokenize='unicode61 remove_diacritics 2'
            );

            CREATE TRIGGER tracks_ai AFTER INSERT ON tracks BEGIN
                INSERT INTO tracks_fts(rowid, title, artist_name, album_title)
                VALUES (new.id, new.title, new.artist_name, new.album_title);
            END;

            INSERT INTO tracks
                (id, title, artist_name, album_id, album_title, cover_id, duration_secs, audio_quality, explicit)
            VALUES
                (1, 'Legacy Song', 'Legacy Artist', 10, 'Legacy Album', NULL, 180, 'LOSSLESS', 0);
            ",
        )
        .unwrap();

        let mut db = CacheDb { conn };
        db.configure_pragmas().unwrap();
        db.migrate().unwrap();
        // La migration doit être idempotente sur une base déjà migrée.
        db.migrate().unwrap();

        {
            let mut stmt = db
                .conn
                .prepare(
                    "SELECT COUNT(*) FROM pragma_table_info('tracks') WHERE name = 'artist_id';",
                )
                .unwrap();
            assert!(stmt.step_row().unwrap());
            assert_eq!(stmt.col_i64(0), 1, "colonne artist_id créée");
        }

        let legacy = db.get_track(1).unwrap().expect("piste existante conservée");
        assert_eq!(legacy.title, "Legacy Song");
        assert_eq!(legacy.album_id, Some(10));
        assert_eq!(legacy.artist_id, None, "valeur par défaut 0 -> None");

        let hits = db.search_local_tracks("legacy", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, 1);

        let mut updated = sample_track(1, "Legacy Song", "Legacy Artist", "Legacy Album");
        updated.artist_id = Some(27);
        db.upsert_tracks(&[updated]).unwrap();
        assert_eq!(db.get_track(1).unwrap().unwrap().artist_id, Some(27));

        let mut fresh = sample_track(2, "Fresh Song", "Fresh Artist", "Fresh Album");
        fresh.artist_id = Some(56);
        db.upsert_tracks(&[fresh]).unwrap();
        assert_eq!(db.get_track(2).unwrap().unwrap().artist_id, Some(56));
        assert_eq!(db.search_local_tracks("fresh", 10).unwrap().len(), 1);
    }

    #[test]
    fn wal_mode_and_fts5_search_work_fast_on_winsqlite3() {
        let tmp = std::env::temp_dir().join(format!(
            "tidenative_cache_test_{}.sqlite3",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&tmp);

        let mut db = CacheDb::open(&tmp).unwrap();
        assert_eq!(db.journal_mode().unwrap().to_lowercase(), "wal");
        assert_eq!(db.busy_timeout_ms().unwrap(), 5000);

        let mut tracks = Vec::with_capacity(500);
        for i in 1..=498 {
            tracks.push(sample_track(
                i,
                &format!("Ambient Study #{i}"),
                "Synthetic Ensemble",
                "Minimal Works",
            ));
        }
        tracks.push(sample_track(
            499,
            "Giorgio by Moroder",
            "Daft Punk",
            "Random Access Memories",
        ));
        tracks.push(sample_track(
            500,
            "So What",
            "Miles Davis",
            "Kind of Blue",
        ));

        db.replace_favorites(&tracks).unwrap();
        let favorites = db.load_favorites().unwrap();
        assert_eq!(favorites.len(), 500);
        assert_eq!(favorites[0].artist_id, Some(3));

        let hits = db.search_local_tracks("morod daft", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, 499);
        assert_eq!(hits[0].artist_id, Some(499 * 3));
        assert_eq!(hits[0].album_id, Some(4990));

        let jazz = db.search_local_tracks("kind blue", 10).unwrap();
        assert_eq!(jazz.len(), 1);
        assert_eq!(jazz[0].artist_name, "Miles Davis");

        drop(db);
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(tmp.with_extension("sqlite3-wal"));
        let _ = std::fs::remove_file(tmp.with_extension("sqlite3-shm"));
    }

    fn activity_track(
        id: u64,
        title: &str,
        artist: &str,
        artist_id: Option<u64>,
    ) -> TrackSummary {
        TrackSummary {
            id,
            title: title.into(),
            artist_name: artist.into(),
            artist_id,
            album_id: Some(id * 10),
            album_title: format!("Album {id}"),
            cover_id: Some(format!("cover-{id}")),
            duration_secs: 240,
            audio_quality: None,
            explicit: false,
        }
    }

    #[test]
    fn listening_stats_keep_all_time_and_thirty_day_totals_accurate() {
        let db = CacheDb::open_in_memory().unwrap();
        let now = unix_now();
        let daft = activity_track(1, "Digital Love", "Daft Punk", Some(27));
        let miles = activity_track(2, "So What", "Miles Davis", None);

        db.record_play_event_at(&daft, 180, now - 5 * SECS_PER_DAY)
            .unwrap();
        db.record_play_event_at(&daft, 200, now - 2 * SECS_PER_DAY)
            .unwrap();
        db.record_play_event_at(&miles, 300, now - 90 * SECS_PER_DAY)
            .unwrap();
        db.record_play_event(&miles, 120).unwrap();

        let all = db.load_listening_stats(None, 10).unwrap();
        assert_eq!(all.total_plays, 4);
        assert_eq!(all.total_secs, 800);
        assert_eq!(all.plays_last_30_days, 3);
        assert_eq!(all.secs_last_30_days, 500);
        assert_eq!(all.unique_artists, 2);
        assert_eq!(all.top_tracks.len(), 2);
        // Égalité de lectures (2 chacune) : départagée par les secondes cumulées.
        assert_eq!(all.top_tracks[0].track.id, 2);
        assert_eq!(all.top_tracks[0].play_count, 2);
        assert_eq!(all.top_tracks[0].total_secs, 420);
        assert_eq!(all.top_tracks[0].track.artist_id, None);
        assert_eq!(all.top_tracks[1].track.id, 1);
        assert_eq!(all.top_tracks[1].total_secs, 380);
        assert_eq!(all.recent_plays.len(), 4);
        assert_eq!(all.recent_plays[0].id, 2, "écoute la plus récente en tête");
        // Même nombre de lectures : Miles Davis passe devant grâce aux secondes.
        assert_eq!(all.top_artists[0].artist_name, "Miles Davis");
        assert_eq!(all.top_artists[0].total_secs, 420);
        assert_eq!(all.top_artists[1].artist_name, "Daft Punk");
        assert_eq!(all.top_artists[1].artist_id, Some(27));

        let window = db.load_listening_stats(Some(30), 10).unwrap();
        assert_eq!(window.total_plays, 4, "totaux toujours all-time");
        assert_eq!(window.total_secs, 800);
        assert_eq!(window.plays_last_30_days, 3);
        assert_eq!(window.secs_last_30_days, 500);
        assert_eq!(window.unique_artists, 2);
        assert_eq!(window.top_tracks.len(), 2);
        assert_eq!(window.top_tracks[0].track.id, 1);
        assert_eq!(window.top_tracks[0].play_count, 2);
        assert_eq!(window.recent_plays.len(), 3);

        let single_day = db.load_listening_stats(Some(1), 10).unwrap();
        assert_eq!(single_day.total_plays, 4);
        assert_eq!(single_day.total_secs, 800);
        assert_eq!(single_day.plays_last_30_days, 3);
        assert_eq!(single_day.unique_artists, 1);
        assert_eq!(single_day.top_tracks.len(), 1);
        assert_eq!(single_day.top_tracks[0].track.id, 2);
        assert_eq!(single_day.top_tracks[0].total_secs, 120);
        assert_eq!(single_day.recent_plays.len(), 1);
        assert_eq!(single_day.top_artists.len(), 1);
        assert_eq!(single_day.top_artists[0].artist_name, "Miles Davis");
        assert_eq!(single_day.top_artists[0].artist_id, None);
    }

    #[test]
    fn listening_stats_rank_tracks_and_group_artists_case_insensitively() {
        let db = CacheDb::open_in_memory().unwrap();
        let now = unix_now();

        for minutes in 1..=3 {
            db.record_play_event_at(
                &activity_track(1, "One More Time", "Daft Punk", Some(27)),
                100,
                now - minutes * 60,
            )
            .unwrap();
        }
        db.record_play_event_at(
            &activity_track(2, "Aerodynamic", "daft punk", Some(27)),
            150,
            now - 10 * 60,
        )
        .unwrap();
        for minutes in [20, 30] {
            db.record_play_event_at(
                &activity_track(3, "Long Take", "Unknown Artist", None),
                600,
                now - minutes * 60,
            )
            .unwrap();
        }

        let stats = db.load_listening_stats(None, 10).unwrap();
        assert_eq!(stats.top_tracks.len(), 3);
        assert_eq!(stats.top_tracks[0].track.id, 1);
        assert_eq!(stats.top_tracks[0].play_count, 3);
        assert_eq!(stats.top_tracks[0].total_secs, 300);
        assert_eq!(stats.top_tracks[0].track.artist_id, Some(27));
        assert_eq!(stats.top_tracks[1].track.id, 3);
        assert_eq!(stats.top_tracks[1].play_count, 2);
        assert_eq!(stats.top_tracks[2].track.id, 2);

        // « Daft Punk » et « daft punk » forment un seul artiste (4 lectures, 450 s).
        assert_eq!(stats.top_artists.len(), 2);
        assert_eq!(stats.top_artists[0].artist_name, "Daft Punk");
        assert_eq!(stats.top_artists[0].artist_id, Some(27));
        assert_eq!(stats.top_artists[0].play_count, 4);
        assert_eq!(stats.top_artists[0].total_secs, 450);
        assert_eq!(stats.top_artists[1].artist_name, "Unknown Artist");
        assert_eq!(stats.top_artists[1].artist_id, None);
        assert_eq!(stats.top_artists[1].play_count, 2);
        assert_eq!(stats.top_artists[1].total_secs, 1_200);
        assert_eq!(stats.unique_artists, 2);

        let top_one = db.load_listening_stats(None, 1).unwrap();
        assert_eq!(top_one.top_tracks.len(), 1);
        assert_eq!(top_one.top_artists.len(), 1);
        assert_eq!(top_one.recent_plays.len(), 1);
        assert_eq!(top_one.recent_plays[0].id, 1);
        assert_eq!(top_one.total_plays, 6);
    }

    #[test]
    fn clear_listening_history_removes_every_event() {
        let db = CacheDb::open_in_memory().unwrap();
        let now = unix_now();
        let track = activity_track(1, "Around the World", "Daft Punk", Some(27));
        db.record_play_event_at(&track, 240, now - 60).unwrap();
        db.record_play_event_at(&track, 240, now - 120).unwrap();

        let before = db.load_listening_stats(None, 10).unwrap();
        assert_eq!(before.total_plays, 2);
        assert_eq!(before.total_secs, 480);
        assert_eq!(before.recent_plays.len(), 2);

        db.clear_listening_history().unwrap();
        let after = db.load_listening_stats(None, 10).unwrap();
        assert_eq!(after, ListeningActivityStats::default());
    }
}
