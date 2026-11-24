use serde::{Deserialize, Serialize};
use tidal_api::lastfm::LastFmCredentials;
use tidal_core::TrackSummary;

pub const LASTFM_SECRET_NAME: &str = "lastfm_session";
pub const MIN_SCROBBLE_DURATION_MS: u64 = 30_000;
/// Seuil maximal entre deux ticks `PositionUpdated` continus (au-delà, il s'agit d'un seek).
pub const MAX_CONTINUOUS_TICK_DELTA_MS: u64 = 2_500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ActivityPeriod {
    #[default]
    AllTime,
    Last30Days,
    Last7Days,
}

impl ActivityPeriod {
    pub fn window_days(self) -> Option<u32> {
        match self {
            Self::AllTime => None,
            Self::Last30Days => Some(30),
            Self::Last7Days => Some(7),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LastFmStatus {
    pub connected: bool,
    pub username: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct ActiveTrackSession {
    pub track: TrackSummary,
    pub started_at: i64,
    pub last_position_ms: Option<u64>,
    pub listened_ms: u64,
    pub persisted_secs: u32,
    pub event_id: Option<i64>,
}

#[derive(Debug, Default)]
pub struct ActivityState {
    pub lastfm_creds: Option<LastFmCredentials>,
    pub active_track_started_at: Option<i64>,
    pub active_track_id: Option<u64>,
    pub active_track_scrobbled: bool,
    pub active_session: Option<ActiveTrackSession>,
}

impl ActivityState {
    pub fn load_persisted() -> Self {
        let lastfm_creds = tidal_auth::load_named_secret(LASTFM_SECRET_NAME)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str::<LastFmCredentials>(&json).ok());
        Self {
            lastfm_creds,
            active_track_started_at: None,
            active_track_id: None,
            active_track_scrobbled: false,
            active_session: None,
        }
    }

    pub fn save_lastfm_creds(&mut self, creds: LastFmCredentials) {
        if let Ok(json) = serde_json::to_string(&creds) {
            let _ = tidal_auth::save_named_secret(LASTFM_SECRET_NAME, &json);
        }
        self.lastfm_creds = Some(creds);
    }

    pub fn clear_lastfm_creds(&mut self) {
        let _ = tidal_auth::delete_named_secret(LASTFM_SECRET_NAME);
        self.lastfm_creds = None;
    }

    pub fn status(&self, enabled: bool) -> LastFmStatus {
        LastFmStatus {
            connected: self.lastfm_creds.is_some(),
            username: self.lastfm_creds.as_ref().map(|c| c.username.clone()),
            enabled: enabled && self.lastfm_creds.is_some(),
        }
    }

    pub fn mark_track_started(&mut self, track_id: u64) -> i64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.active_track_id = Some(track_id);
        self.active_track_started_at = Some(now);
        self.active_track_scrobbled = false;
        self.active_session = None;
        now
    }

    pub fn begin_track_session(&mut self, track: TrackSummary) -> i64 {
        let now = self.mark_track_started(track.id);
        self.active_session = Some(ActiveTrackSession {
            track,
            started_at: now,
            last_position_ms: Some(0),
            listened_ms: 0,
            persisted_secs: 0,
            event_id: None,
        });
        now
    }

    /// Accumule uniquement le temps de lecture réellement écoulé entre deux positions continues,
    /// en ignorant les sauts liés à un seek manuel.
    pub fn record_position_tick(&mut self, track_id: u64, position_ms: u64) -> u32 {
        let Some(sess) = self.active_session.as_mut() else {
            return 0;
        };
        if sess.track.id != track_id {
            return 0;
        }
        if let Some(prev_ms) = sess.last_position_ms {
            if position_ms > prev_ms {
                let delta = position_ms - prev_ms;
                if delta <= MAX_CONTINUOUS_TICK_DELTA_MS {
                    sess.listened_ms = sess.listened_ms.saturating_add(delta);
                }
            }
        }
        sess.last_position_ms = Some(position_ms);
        (sess.listened_ms / 1000) as u32
    }

    pub fn notify_seek(&mut self, position_ms: u64) {
        if let Some(sess) = self.active_session.as_mut() {
            sess.last_position_ms = Some(position_ms);
        }
    }

    pub fn reset_active_persistence(&mut self) {
        if let Some(sess) = self.active_session.as_mut() {
            sess.listened_ms = 0;
            sess.persisted_secs = 0;
            sess.event_id = None;
        }
    }

    pub fn should_scrobble_now(&self, track_id: u64, position_ms: u64, duration_ms: u64) -> bool {
        if self.active_track_id != Some(track_id) || self.active_track_scrobbled {
            return false;
        }
        if !should_scrobble_lastfm(duration_ms) {
            return false;
        }
        let threshold_ms = (duration_ms / 2).min(240_000);
        let effective_ms = match self.active_session.as_ref() {
            Some(sess) if sess.track.id == track_id => sess.listened_ms,
            _ => position_ms,
        };
        effective_ms >= threshold_ms
    }
}

pub fn should_scrobble_lastfm(duration_ms: u64) -> bool {
    duration_ms >= MIN_SCROBBLE_DURATION_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrobble_threshold_requires_30s_duration_and_half_played() {
        let mut act = ActivityState::default();
        act.mark_track_started(42);
        assert!(!act.should_scrobble_now(42, 20_000, 25_000));
        assert!(!act.should_scrobble_now(42, 80_000, 200_000));
        assert!(act.should_scrobble_now(42, 100_000, 200_000));
        act.active_track_scrobbled = true;
        assert!(!act.should_scrobble_now(42, 120_000, 200_000));
    }

    #[test]
    fn accumulates_only_continuous_listening_time_and_ignores_seeks() {
        let mut act = ActivityState::default();
        let track = TrackSummary {
            id: 7,
            title: "Test".into(),
            artist_name: "Artist".into(),
            artist_id: Some(1),
            album_id: Some(1),
            album_title: "Album".into(),
            cover_id: None,
            duration_secs: 240,
            audio_quality: None,
            explicit: false,
        };
        act.begin_track_session(track);
        assert_eq!(act.record_position_tick(7, 500), 0);
        assert_eq!(act.record_position_tick(7, 1_500), 1);
        assert_eq!(act.record_position_tick(7, 3_000), 3);
        // Saut de seek en avant vers 120s : ne doit pas ajouter 117s au temps écouté
        assert_eq!(act.record_position_tick(7, 120_000), 3);
        assert_eq!(act.record_position_tick(7, 121_200), 4);
    }
}
