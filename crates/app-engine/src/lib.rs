pub mod activity;
pub mod library;
pub mod playlist_io;
pub mod queue;
pub mod runtime;

use crate::activity::{ActivityPeriod, ActivityState, LastFmStatus};
use crate::library::{
    LibraryState, curate_diverse_discover_tracks, filter_tracks, filter_tracks_with_positions,
    pick_diverse_seed_artists, pick_diverse_seed_tracks,
};
pub use crate::playlist_io::{PlaylistExportFormat, PlaylistExportPayload, PlaylistImportResult};
use crate::queue::{QueueState, TrackStep, next_track_index_for_transition, next_track_step, previous_track_step, track_finished_step};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use parking_lot::{Mutex, RwLock};
use std::sync::Arc;
use std::time::Instant;
use tidal_api::TidalApiClient;
use tidal_audio_wasapi::{WasapiStreamConfig, enumerate_output_devices, live_audio_levels, set_realtime_volume};
use tidal_auth::client::TidalClient;
use tidal_auth::store;
use tidal_auth::{ClientCredentials, TokenSet};
use tidal_cache::{CacheDb, CoverCache, SnapshotStore, default_covers_dir, default_db_path, default_snapshot_path};
use tidal_core::{
    AlbumSummary, AppPreferences, ArtistDetailData, ArtistSummary, AudioDeviceDescriptor,
    CoverPalette, ExploreHubData, ListeningActivityStats, MixSummary, PlaybackState,
    PlaylistFolder, PlaylistSummary, RepeatMode, SearchResults, SearchSuggestions,
    StreamFormatInfo, TopHit, TrackCredits, TrackLyrics, TrackSummary, UiSnapshot,
};
use tidal_platform_win::{SmtcCommand, SmtcController, TrayCommand, WebTidalSession};
use tidal_player::{PlayerHandle, spawn_player_actor};
use tokio::sync::{broadcast, mpsc};

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("non connecté à TIDAL")]
    NotAuthenticated,

    #[error("erreur API TIDAL : {0}")]
    Api(String),

    #[error("erreur cache local : {0}")]
    Cache(String),

    #[error("erreur audio : {0}")]
    Audio(String),

    #[error("paramètre invalide : {0}")]
    InvalidInput(String),
}

pub type EngineResult<T> = std::result::Result<T, EngineError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum TrayUiAction {
    ShowWindow,
    QuitApp,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TelemetryFrame {
    pub levels: [f32; 5],
    pub position_ms: u64,
    pub duration_ms: u64,
    pub buffered_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BootstrapState {
    pub authenticated: bool,
    pub user_id: Option<u64>,
    pub country_code: Option<String>,
    pub preferences: AppPreferences,
    pub devices: Vec<AudioDeviceDescriptor>,
    pub playback_state: PlaybackState,
    pub active_format: Option<StreamFormatInfo>,
    pub current_track: Option<TrackSummary>,
    pub current_palette: CoverPalette,
    pub queue: Vec<TrackSummary>,
    pub queue_index: Option<u32>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub favorites: Vec<TrackSummary>,
    pub favorite_ids: Vec<u64>,
    pub playlists: Vec<PlaylistSummary>,
    pub playlist_folders: Vec<PlaylistFolder>,
    pub favorite_albums: Vec<AlbumSummary>,
    pub favorite_album_ids: Vec<u64>,
    pub favorite_artists: Vec<ArtistSummary>,
    pub favorite_artist_ids: Vec<u64>,
    pub user_mixes: Vec<MixSummary>,
    pub discover_tracks: Vec<TrackSummary>,
    /// `true` tant que la sélection d'accueil n'a pas été renouvelée pour la session courante.
    pub discover_renewal_pending: bool,
    pub recent_tracks: Vec<TrackSummary>,
    pub recent_searches: Vec<String>,
    pub explore_hub: Option<ExploreHubData>,
    pub lastfm: LastFmStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AlbumDetailData {
    pub album: AlbumSummary,
    pub tracks: Vec<TrackSummary>,
    pub is_favorite: bool,
    pub similar_albums: Vec<AlbumSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlaylistDetailData {
    pub playlist: PlaylistSummary,
    pub tracks: Vec<TrackSummary>,
    pub tidal_indices: Vec<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct HybridSearchResults {
    pub seq: u64,
    pub query: String,
    pub local_tracks: Vec<TrackSummary>,
    pub remote: SearchResults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LibrarySyncPayload {
    pub favorites: Vec<TrackSummary>,
    pub favorite_ids: Vec<u64>,
    pub playlists: Vec<PlaylistSummary>,
    pub playlist_folders: Vec<PlaylistFolder>,
    pub favorite_albums: Vec<AlbumSummary>,
    pub favorite_album_ids: Vec<u64>,
    pub favorite_artists: Vec<ArtistSummary>,
    pub favorite_artist_ids: Vec<u64>,
    pub user_mixes: Vec<MixSummary>,
    pub discover_tracks: Vec<TrackSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", content = "payload")]
pub enum EngineEvent {
    PlaybackStateChanged {
        state: PlaybackState,
    },
    TrackChanged {
        track: Option<TrackSummary>,
        palette: CoverPalette,
        queue_index: Option<u32>,
        duration_ms: u64,
    },
    QueueUpdated {
        queue: Vec<TrackSummary>,
        queue_index: Option<u32>,
    },
    FormatNegotiated {
        format: StreamFormatInfo,
    },
    DevicesEnumerated {
        devices: Vec<AudioDeviceDescriptor>,
    },
    LibrarySynced {
        payload: LibrarySyncPayload,
    },
    PreferencesUpdated {
        preferences: AppPreferences,
    },
    AuthChanged {
        authenticated: bool,
        user_id: Option<u64>,
        country_code: Option<String>,
    },
    ErrorBanner {
        message: String,
    },
    StatusBanner {
        message: String,
    },
    TrayAction {
        action: TrayUiAction,
    },
}

pub(crate) struct PlaybackStateInner {
    pub queue: QueueState,
    pub state: PlaybackState,
    pub active_format: Option<StreamFormatInfo>,
    pub current_palette: CoverPalette,
    pub devices: Vec<AudioDeviceDescriptor>,
    pub muted_prev_volume: Option<f32>,
}

struct SessionRuntime {
    api: Arc<TidalApiClient>,
    player: PlayerHandle,
    user_id: Option<u64>,
    country_code: Option<String>,
}

enum PendingListeningWrite {
    Insert {
        track: TrackSummary,
        played_secs: u32,
        started_at: i64,
    },
    Update {
        event_id: i64,
        played_secs: u32,
    },
}

pub struct AppEngine {
    rt: tokio::runtime::Runtime,
    http: reqwest::Client,
    snapshot_store: SnapshotStore,
    db: Arc<Mutex<Option<CacheDb>>>,
    cover_cache: Arc<CoverCache>,
    pub(crate) playback: RwLock<PlaybackStateInner>,
    library: RwLock<LibraryState>,
    preferences: RwLock<AppPreferences>,
    session: RwLock<Option<SessionRuntime>>,
    pub(crate) activity: Arc<Mutex<ActivityState>>,
    smtc: Mutex<Option<SmtcController>>,
    events_tx: broadcast::Sender<EngineEvent>,
    telemetry_active: AtomicBool,
    atomic_position_ms: AtomicU64,
    atomic_duration_ms: AtomicU64,
    atomic_buffered_ms: AtomicU64,
    snapshot_persist_seq: AtomicU64,
    last_position_persist_epoch_secs: AtomicU64,
    active_search_hybrid: Mutex<Option<tokio::task::AbortHandle>>,
    active_search_suggestions: Mutex<Option<tokio::task::AbortHandle>>,
}

impl AppEngine {
    pub fn boot() -> Arc<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(4)
            .thread_name("tidenative-engine")
            .build()
            .expect("failed to build AppEngine tokio runtime");

        let snapshot_store = SnapshotStore::new(default_snapshot_path());
        let snap = snapshot_store.load_or_default();

        let http = tidal_auth::http_client().unwrap_or_else(|_| reqwest::Client::new());
        let cover_cache = Arc::new(CoverCache::new(default_covers_dir(), http.clone()));
        let db = CacheDb::open(&default_db_path()).ok();

        let devices = enumerate_output_devices().unwrap_or_default();
        let activity_state = ActivityState::load_persisted();

        let mut queue = QueueState::default();
        queue.play_history = snap.recent_tracks.clone();
        if let Some(ref session_pb) = snap.playback_session {
            queue.restore_from_session(session_pb);
        }
        let initial_pos = queue.position_ms;
        let initial_dur = queue.duration_ms;

        let mut library_state = LibraryState::default();
        library_state.set_favorites(snap.favorites.clone());
        library_state.playlists = snap.playlists.clone();
        library_state.playlist_folders = snap.playlist_folders.clone();
        library_state.set_favorite_albums(snap.favorite_albums.clone());
        library_state.set_favorite_artists(snap.favorite_artists.clone());
        library_state.user_mixes = snap.user_mixes.clone();
        library_state.discover_tracks = snap.discover_tracks.clone();
        library_state.discover_refreshed_at = snap.discover_refreshed_at;
        library_state.discover_rotation = snap.discover_rotation;
        // Une sélection persistée vient forcément d'une session précédente :
        // on la renouvelle au démarrage pour que l'accueil change à chaque lancement.
        library_state.discover_renewal_pending = !snap.discover_tracks.is_empty();
        library_state.recent_searches = snap.recent_searches.clone();

        set_realtime_volume(snap.preferences.volume, snap.preferences.bit_perfect_lock);

        let (events_tx, _) = broadcast::channel(128);

        let engine = Arc::new(Self {
            rt,
            http,
            snapshot_store,
            db: Arc::new(Mutex::new(db)),
            cover_cache,
            playback: RwLock::new(PlaybackStateInner {
                queue,
                state: PlaybackState::Stopped,
                active_format: None,
                current_palette: CoverPalette::default(),
                devices,
                muted_prev_volume: None,
            }),
            library: RwLock::new(library_state),
            preferences: RwLock::new(snap.preferences.clone()),
            session: RwLock::new(None),
            activity: Arc::new(Mutex::new(activity_state)),
            smtc: Mutex::new(None),
            events_tx,
            telemetry_active: AtomicBool::new(false),
            atomic_position_ms: AtomicU64::new(initial_pos),
            atomic_duration_ms: AtomicU64::new(initial_dur),
            atomic_buffered_ms: AtomicU64::new(0),
            snapshot_persist_seq: AtomicU64::new(0),
            last_position_persist_epoch_secs: AtomicU64::new(0),
            active_search_hybrid: Mutex::new(None),
            active_search_suggestions: Mutex::new(None),
        });

        // Try restoring saved TIDAL token synchronously (fast local store read).
        if let Ok(Some(token)) = store::load() {
            engine.attach_authenticated_token(token, snap.user_id, snap.country_code);
        }

        // Warm initial cover palette asynchronously if a track was restored.
        if let Some(cover_id) = engine
            .playback
            .read()
            .queue
            .current_track
            .as_ref()
            .and_then(|t| t.cover_id.clone())
        {
            let eng = Arc::clone(&engine);
            engine.rt.spawn(async move {
                let palette = eng.cover_cache.get_palette(&cover_id).await;
                let (track, queue_index, duration_ms) = {
                    let mut pb = eng.playback.write();
                    pb.current_palette = palette.clone();
                    (
                        pb.queue.current_track.clone(),
                        pb.queue.queue_index.map(|i| i as u32),
                        pb.queue.duration_ms,
                    )
                };
                eng.emit(EngineEvent::TrackChanged {
                    track,
                    palette,
                    queue_index,
                    duration_ms,
                });
            });
        }

        engine
    }

    pub fn runtime(&self) -> &tokio::runtime::Runtime {
        &self.rt
    }

    pub fn cover_cache(&self) -> Arc<CoverCache> {
        Arc::clone(&self.cover_cache)
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<EngineEvent> {
        self.events_tx.subscribe()
    }

    pub fn emit(&self, event: EngineEvent) {
        let _ = self.events_tx.send(event);
    }

    pub fn is_telemetry_active(&self) -> bool {
        self.telemetry_active.load(Ordering::Relaxed)
    }

    pub fn telemetry_snapshot(&self) -> TelemetryFrame {
        TelemetryFrame {
            levels: live_audio_levels(),
            position_ms: self.atomic_position_ms.load(Ordering::Relaxed),
            duration_ms: self.atomic_duration_ms.load(Ordering::Relaxed),
            buffered_ms: self.atomic_buffered_ms.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn update_atomic_position(&self, pos_ms: u64, dur_ms: u64) {
        self.atomic_position_ms.store(pos_ms, Ordering::Relaxed);
        if dur_ms > 0 {
            self.atomic_duration_ms.store(dur_ms, Ordering::Relaxed);
        }
    }

    pub(crate) fn update_atomic_buffered(&self, buffered_ms: u64) {
        self.atomic_buffered_ms.store(buffered_ms, Ordering::Relaxed);
    }

    async fn run_db<T, F>(&self, op: F) -> Option<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut CacheDb) -> T + Send + 'static,
    {
        let db = Arc::clone(&self.db);
        self.rt
            .spawn_blocking(move || {
                let mut guard = db.lock();
                guard.as_mut().map(op)
            })
            .await
            .ok()
            .flatten()
    }

    pub(crate) fn apply_playback_state(&self, state: PlaybackState) {
        let active = runtime::is_active_playback(&state);
        self.telemetry_active.store(active, Ordering::Relaxed);
        if !active {
            self.flush_active_listening_progress(true);
        }
        {
            let mut pb = self.playback.write();
            pb.state = state.clone();
        }
        if let Some(smtc) = self.smtc.lock().as_ref() {
            smtc.update_playback_state(&state);
        }
    }

    fn take_pending_listening_write(&self, force: bool) -> Option<PendingListeningWrite> {
        let mut act = self.activity.lock();
        let sess = act.active_session.as_mut()?;
        let played_secs = (sess.listened_ms / 1000) as u32;
        if played_secs == 0 {
            return None;
        }
        match sess.event_id {
            None => {
                if sess.insert_in_flight {
                    return None;
                }
                sess.insert_in_flight = true;
                Some(PendingListeningWrite::Insert {
                    track: sess.track.clone(),
                    played_secs,
                    started_at: sess.started_at,
                })
            }
            Some(event_id) => {
                let should_update = (force && played_secs > sess.persisted_secs)
                    || played_secs >= sess.persisted_secs.saturating_add(2);
                if should_update {
                    Some(PendingListeningWrite::Update {
                        event_id,
                        played_secs,
                    })
                } else {
                    None
                }
            }
        }
    }

    fn execute_listening_write(
        db: &Arc<Mutex<Option<CacheDb>>>,
        activity: &Arc<Mutex<ActivityState>>,
        op: PendingListeningWrite,
    ) {
        match op {
            PendingListeningWrite::Insert {
                track,
                played_secs,
                started_at,
            } => {
                let inserted_id = db
                    .lock()
                    .as_ref()
                    .and_then(|d| d.insert_play_event_at(&track, played_secs, started_at).ok());
                let mut act = activity.lock();
                if let Some(sess) = act.active_session.as_mut() {
                    if sess.track.id == track.id && sess.started_at == started_at {
                        sess.insert_in_flight = false;
                        if let Some(id) = inserted_id {
                            sess.event_id = Some(id);
                            sess.persisted_secs = played_secs;
                        }
                    }
                }
            }
            PendingListeningWrite::Update {
                event_id,
                played_secs,
            } => {
                let updated = db
                    .lock()
                    .as_ref()
                    .and_then(|d| d.update_play_event_duration(event_id, played_secs).ok())
                    .is_some();
                if updated {
                    let mut act = activity.lock();
                    if let Some(sess) = act.active_session.as_mut() {
                        if sess.event_id == Some(event_id) {
                            sess.persisted_secs = played_secs;
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn flush_active_listening_progress(&self, force: bool) {
        let Some(op) = self.take_pending_listening_write(force) else {
            return;
        };
        let db = Arc::clone(&self.db);
        let activity = Arc::clone(&self.activity);
        self.rt.spawn_blocking(move || {
            Self::execute_listening_write(&db, &activity, op);
        });
    }

    pub(crate) async fn flush_active_listening_progress_async(&self, force: bool) {
        let Some(op) = self.take_pending_listening_write(force) else {
            return;
        };
        let db = Arc::clone(&self.db);
        let activity = Arc::clone(&self.activity);
        let _ = self
            .rt
            .spawn_blocking(move || {
                Self::execute_listening_write(&db, &activity, op);
            })
            .await;
    }

    pub(crate) fn set_active_format(&self, format: Option<StreamFormatInfo>) {
        self.playback.write().active_format = format;
    }

    pub(crate) fn set_devices(&self, devices: Vec<AudioDeviceDescriptor>) {
        self.playback.write().devices = devices;
    }

    pub fn preferences(&self) -> AppPreferences {
        self.preferences.read().clone()
    }

    pub fn bootstrap_state(&self) -> BootstrapState {
        let prefs = self.preferences();
        let (authenticated, user_id, country_code) = {
            let sess = self.session.read();
            match sess.as_ref() {
                Some(s) => (true, s.user_id, s.country_code.clone()),
                None => (false, None, None),
            }
        };
        let pb = self.playback.read();
        let lib = self.library.read();
        let lastfm = self.activity.lock().status(prefs.lastfm_enabled);

        BootstrapState {
            authenticated,
            user_id,
            country_code,
            preferences: prefs,
            devices: pb.devices.clone(),
            playback_state: pb.state.clone(),
            active_format: pb.active_format.clone(),
            current_track: pb.queue.current_track.clone(),
            current_palette: pb.current_palette.clone(),
            queue: pb.queue.queue.clone(),
            queue_index: pb.queue.queue_index.map(|i| i as u32),
            position_ms: pb.queue.position_ms,
            duration_ms: pb.queue.duration_ms,
            favorites: lib.favorites.clone(),
            favorite_ids: lib.favorite_ids.iter().copied().collect(),
            playlists: lib.playlists.clone(),
            playlist_folders: lib.playlist_folders.clone(),
            favorite_albums: lib.favorite_albums.clone(),
            favorite_album_ids: lib.favorite_album_ids.iter().copied().collect(),
            favorite_artists: lib.favorite_artists.clone(),
            favorite_artist_ids: lib.favorite_artist_ids.iter().copied().collect(),
            user_mixes: lib.user_mixes.clone(),
            discover_tracks: lib.discover_tracks.clone(),
            discover_renewal_pending: lib.discover_renewal_pending,
            recent_tracks: pb.queue.play_history.clone(),
            recent_searches: lib.recent_searches.clone(),
            explore_hub: lib.explore_hub.clone(),
            lastfm,
        }
    }

    pub fn attach_smtc(self: &Arc<Self>, hwnd: isize) {
        let (tx, mut rx) = mpsc::unbounded_channel::<SmtcCommand>();
        if let Ok(controller) = SmtcController::attach(hwnd, tx) {
            {
                let pb = self.playback.read();
                controller.update_playback_state(&pb.state);
                if let Some(track) = pb.queue.current_track.as_ref() {
                    controller.update_metadata(track);
                }
            }
            *self.smtc.lock() = Some(controller);
            let engine = Arc::clone(self);
            self.rt.spawn(async move {
                while let Some(cmd) = rx.recv().await {
                    engine.dispatch_smtc_command(cmd).await;
                }
            });
        }
    }

    pub async fn dispatch_smtc_command(self: &Arc<Self>, cmd: SmtcCommand) {
        match cmd {
            SmtcCommand::Play => {
                let is_playing = matches!(
                    self.playback.read().state,
                    PlaybackState::Playing
                );
                if !is_playing {
                    let _ = self.toggle_play_pause().await;
                }
            }
            SmtcCommand::Pause => {
                let is_playing = matches!(
                    self.playback.read().state,
                    PlaybackState::Playing | PlaybackState::Buffering
                );
                if is_playing {
                    let _ = self.toggle_play_pause().await;
                }
            }
            SmtcCommand::Next => {
                let _ = self.next_track().await;
            }
            SmtcCommand::Previous => {
                let _ = self.prev_track().await;
            }
            SmtcCommand::Stop => {
                if let Some(player) = self.player_handle() {
                    player.stop();
                }
            }
        }
    }

    pub async fn dispatch_tray_command(self: &Arc<Self>, cmd: TrayCommand) {
        match cmd {
            TrayCommand::ShowWindow => {
                self.emit(EngineEvent::TrayAction {
                    action: TrayUiAction::ShowWindow,
                });
            }
            TrayCommand::TogglePlayPause => {
                let _ = self.toggle_play_pause().await;
            }
            TrayCommand::Next => {
                let _ = self.next_track().await;
            }
            TrayCommand::Previous => {
                let _ = self.prev_track().await;
            }
            TrayCommand::Quit => {
                self.persist_snapshot();
                self.emit(EngineEvent::TrayAction {
                    action: TrayUiAction::QuitApp,
                });
            }
        }
    }

    fn attach_authenticated_token(
        self: &Arc<Self>,
        mut token: TokenSet,
        user_id: Option<u64>,
        country_code: Option<String>,
    ) {
        if token.user_id.is_none() {
            token.user_id = user_id;
        }
        if token.country_code.is_none() {
            token.country_code = country_code.clone();
        }
        let creds = ClientCredentials::load().unwrap_or_default();
        let client = Arc::new(TidalClient::new(self.http.clone(), creds, token));
        let api = Arc::new(TidalApiClient::new(Arc::clone(&client)));
        let prefs = self.preferences();
        let config = WasapiStreamConfig {
            device_id: prefs.device_id.clone(),
            exclusive: prefs.exclusive_mode,
            fallback_to_shared: prefs.fallback_to_shared,
            target_buffer_ms: 20,
        };

        let _enter = self.rt.enter();
        let (player, evt_rx) = spawn_player_actor(client, config);
        player.set_volume(prefs.volume, prefs.bit_perfect_lock);
        player.set_normalization(prefs.normalize_volume);
        player.set_replay_gain_mode(prefs.replay_gain_mode);
        player.set_gapless(prefs.gapless_playback);

        {
            let mut sess = self.session.write();
            *sess = Some(SessionRuntime {
                api,
                player,
                user_id,
                country_code,
            });
        }

        runtime::spawn_player_event_loop(Arc::clone(self), evt_rx);
    }

    fn api_handle(&self) -> EngineResult<Arc<TidalApiClient>> {
        self.session
            .read()
            
            .as_ref()
            .map(|s| Arc::clone(&s.api))
            .ok_or(EngineError::NotAuthenticated)
    }

    fn player_handle(&self) -> Option<PlayerHandle> {
        self.session
            .read()
            
            .as_ref()
            .map(|s| s.player.clone())
    }

    fn build_snapshot(&self) -> UiSnapshot {
        self.flush_active_listening_progress(true);
        let prefs = self.preferences();
        let (user_id, country_code) = {
            let sess = self.session.read();
            sess.as_ref()
                .map(|s| (s.user_id, s.country_code.clone()))
                .unwrap_or((None, None))
        };
        let (recent_tracks, playback_session) = {
            let pb = self.playback.read();
            (pb.queue.play_history.clone(), pb.queue.export_session())
        };
        let (
            favorites,
            playlists,
            playlist_folders,
            favorite_albums,
            favorite_artists,
            user_mixes,
            discover_tracks,
            discover_refreshed_at,
            discover_rotation,
            recent_searches,
        ) = {
            let lib = self.library.read();
            (
                lib.favorites.clone(),
                lib.playlists.clone(),
                lib.playlist_folders.clone(),
                lib.favorite_albums.clone(),
                lib.favorite_artists.clone(),
                lib.user_mixes.clone(),
                lib.discover_tracks.clone(),
                lib.discover_refreshed_at,
                lib.discover_rotation,
                lib.recent_searches.clone(),
            )
        };
        UiSnapshot {
            user_id,
            country_code,
            preferences: prefs,
            favorites,
            playlists,
            playlist_folders,
            favorite_albums,
            favorite_artists,
            recent_searches,
            recent_tracks,
            user_mixes,
            discover_tracks,
            discover_refreshed_at,
            discover_rotation,
            playback_session,
        }
    }

    pub fn persist_snapshot(&self) {
        let snap = self.build_snapshot();
        let _ = self.snapshot_store.save(&snap);
    }

    pub fn schedule_persist_snapshot(self: &Arc<Self>) {
        let seq = self.snapshot_persist_seq.fetch_add(1, Ordering::AcqRel) + 1;
        let engine = Arc::clone(self);
        self.rt.spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(350)).await;
            if engine.snapshot_persist_seq.load(Ordering::Acquire) != seq {
                return;
            }
            let snap = engine.build_snapshot();
            let store = engine.snapshot_store.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let _ = store.save(&snap);
            })
            .await;
        });
    }

    pub(crate) fn maybe_persist_playback_position(self: &Arc<Self>) {
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let prev = self.last_position_persist_epoch_secs.load(Ordering::Relaxed);
        if now_secs.saturating_sub(prev) >= 10
            && self
                .last_position_persist_epoch_secs
                .compare_exchange(prev, now_secs, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        {
            self.schedule_persist_snapshot();
        }
    }

    pub async fn complete_web_login(self: &Arc<Self>, web: WebTidalSession) -> EngineResult<BootstrapState> {
        let token = TokenSet::from_web_session(web.access_token, web.refresh_token, web.expires_in);
        store::save(&token).map_err(|e| EngineError::Api(e.to_string()))?;
        self.attach_authenticated_token(token, None, None);

        let api = self.api_handle()?;
        let session = api
            .ensure_session()
            .await
            .map_err(|e| EngineError::Api(e.to_string()))?;

        {
            let mut guard = self.session.write();
            if let Some(s) = guard.as_mut() {
                s.user_id = Some(session.user_id);
                s.country_code = Some(session.country_code.clone());
            }
        }

        self.emit(EngineEvent::AuthChanged {
            authenticated: true,
            user_id: Some(session.user_id),
            country_code: Some(session.country_code),
        });
        let _ = self.refresh_library().await;
        self.persist_snapshot();
        Ok(self.bootstrap_state())
    }

    pub async fn logout(self: &Arc<Self>) -> EngineResult<()> {
        if let Some(player) = self.player_handle() {
            player.stop();
        }
        let _ = store::delete();
        {
            let mut sess = self.session.write();
            *sess = None;
        }
        {
            let mut lib = self.library.write();
            *lib = LibraryState::default();
        }
        self.apply_playback_state(PlaybackState::Stopped);
        self.persist_snapshot();
        self.emit(EngineEvent::AuthChanged {
            authenticated: false,
            user_id: None,
            country_code: None,
        });
        Ok(())
    }

    pub async fn play_tracks(
        self: &Arc<Self>,
        tracks: Vec<TrackSummary>,
        start_index: usize,
    ) -> EngineResult<()> {
        let prefs = self.preferences();
        let (filtered, positions) =
            filter_tracks_with_positions(tracks, prefs.allow_explicit, prefs.allow_ai_content);
        if filtered.is_empty() {
            return Err(EngineError::InvalidInput("liste de pistes vide".into()));
        }
        let mapped_start = positions
            .iter()
            .position(|&orig_idx| orig_idx >= start_index)
            .unwrap_or(filtered.len().saturating_sub(1));
        let (track, queue_snapshot, queue_index) = {
            let mut pb = self.playback.write();
            let selected = pb
                .queue
                .set_queue_and_select(filtered, mapped_start, prefs.shuffle_enabled)
                .ok_or_else(|| EngineError::InvalidInput("index de départ invalide".into()))?;
            (
                selected,
                pb.queue.queue.clone(),
                pb.queue.queue_index.map(|i| i as u32),
            )
        };
        self.emit(EngineEvent::QueueUpdated {
            queue: queue_snapshot,
            queue_index,
        });
        self.start_track_playback(track, queue_index).await
    }

    pub async fn play_queue_index(self: &Arc<Self>, index: usize) -> EngineResult<()> {
        let (track, queue_index) = {
            let mut pb = self.playback.write();
            let selected = pb
                .queue
                .select_index(index)
                .ok_or_else(|| EngineError::InvalidInput("index hors file".into()))?;
            (selected, pb.queue.queue_index.map(|i| i as u32))
        };
        self.start_track_playback(track, queue_index).await
    }

    pub async fn play_track_by_id(self: &Arc<Self>, track_id: u64) -> EngineResult<()> {
        let existing_idx = {
            let pb = self.playback.read();
            pb.queue.queue.iter().position(|t| t.id == track_id)
        };
        if let Some(idx) = existing_idx {
            return self.play_queue_index(idx).await;
        }
        let found = {
            let lib = self.library.read();
            lib.favorites
                .iter()
                .chain(lib.discover_tracks.iter())
                .find(|t| t.id == track_id)
                .cloned()
        };
        let found = match found {
            Some(t) => Some(t),
            None => self
                .run_db(move |db| db.get_track(track_id).ok().flatten())
                .await
                .flatten(),
        };
        let track = found.ok_or_else(|| EngineError::InvalidInput("piste introuvable".into()))?;
        self.play_tracks(vec![track], 0).await
    }

    async fn start_track_playback(
        self: &Arc<Self>,
        track: TrackSummary,
        queue_index: Option<u32>,
    ) -> EngineResult<()> {
        self.start_track_playback_at(track, queue_index, None).await
    }

    async fn start_track_playback_at(
        self: &Arc<Self>,
        track: TrackSummary,
        queue_index: Option<u32>,
        start_position_ms: Option<u64>,
    ) -> EngineResult<()> {
        let player = self.player_handle().ok_or(EngineError::NotAuthenticated)?;
        let prefs = self.preferences();
        let duration_ms = u64::from(track.duration_secs) * 1000;
        let initial_ms = start_position_ms.unwrap_or(0);
        self.update_atomic_position(initial_ms, duration_ms);
        self.update_atomic_buffered(0);

        if initial_ms > 0 {
            player.load_track_at(track.id, prefs.audio_quality, initial_ms);
        } else {
            player.load_track(track.id, prefs.audio_quality);
        }
        self.sync_gapless_and_prefetch(&player, &prefs);

        if let Some(ctrl) = self.smtc.lock().as_ref() {
            ctrl.update_metadata(&track);
        }
        self.flush_active_listening_progress(true);
        {
            let mut act = self.activity.lock();
            act.begin_track_session(track.clone());
            if prefs.lastfm_enabled {
                if let Some(creds) = act.lastfm_creds.clone() {
                    let http = self.http.clone();
                    let t_clone = track.clone();
                    self.rt.spawn(async move {
                        let _ = tidal_api::lastfm::update_now_playing(&http, &creds, &t_clone).await;
                    });
                }
            }
        }

        let current_palette = self
            .playback
            .read()
            
            .current_palette
            .clone();
        self.emit(EngineEvent::TrackChanged {
            track: Some(track.clone()),
            palette: current_palette,
            queue_index,
            duration_ms,
        });

        // Resolve cover palette asynchronously without blocking audio start.
        if let Some(cover_id) = track.cover_id.clone() {
            let eng = Arc::clone(self);
            let track_id = track.id;
            self.rt.spawn(async move {
                let palette = eng.cover_cache.get_palette(&cover_id).await;
                let still_current = {
                    let mut pb = eng.playback.write();
                    if pb.queue.current_track.as_ref().map(|t| t.id) == Some(track_id) {
                        pb.current_palette = palette.clone();
                        Some((pb.queue.current_track.clone(), pb.queue.queue_index.map(|i| i as u32), pb.queue.duration_ms))
                    } else {
                        None
                    }
                };
                if let Some((cur, q_idx, dur)) = still_current {
                    eng.emit(EngineEvent::TrackChanged {
                        track: cur,
                        palette,
                        queue_index: q_idx,
                        duration_ms: dur,
                    });
                }
            });
        }

        self.persist_snapshot();
        Ok(())
    }

    fn sync_gapless_and_prefetch(&self, player: &PlayerHandle, prefs: &AppPreferences) {
        let next_id = {
            let pb = self.playback.read();
            pb.queue.gapless_next_track_id(prefs.repeat_mode)
        };
        if prefs.gapless_playback {
            player.queue_gapless_next(next_id, prefs.audio_quality);
        } else {
            player.queue_gapless_next(None, prefs.audio_quality);
        }
        if let Some(id) = next_id {
            player.prefetch_track(id, prefs.audio_quality);
        }
    }

    pub async fn toggle_play_pause(self: &Arc<Self>) -> EngineResult<()> {
        let player = self.player_handle().ok_or(EngineError::NotAuthenticated)?;
        let (state, resume_info) = {
            let mut pb = self.playback.write();
            let resume = if matches!(pb.state, PlaybackState::Stopped | PlaybackState::Error(_)) {
                pb.queue.current_track.clone().map(|t| {
                    let seek = pb.queue.pending_resume_seek_ms.take();
                    (t, pb.queue.queue_index.map(|i| i as u32), seek)
                })
            } else {
                None
            };
            (pb.state.clone(), resume)
        };

        match state {
            PlaybackState::Playing | PlaybackState::Buffering => player.pause(),
            PlaybackState::Paused => player.play(),
            PlaybackState::Stopped | PlaybackState::Error(_) => {
                if let Some((track, q_idx, resume_seek)) = resume_info {
                    self.start_track_playback_at(track, q_idx, resume_seek).await?;
                } else {
                    let has_queue = !self.playback.read().queue.queue.is_empty();
                    if has_queue {
                        self.play_queue_index(0).await?;
                    } else {
                        let fallback_tracks = {
                            let pb = self.playback.read();
                            if !pb.queue.play_history.is_empty() {
                                pb.queue.play_history.clone()
                            } else {
                                drop(pb);
                                let lib = self.library.read();
                                if !lib.discover_tracks.is_empty() {
                                    lib.discover_tracks.clone()
                                } else {
                                    lib.favorites.clone()
                                }
                            }
                        };
                        if !fallback_tracks.is_empty() {
                            self.play_tracks(fallback_tracks, 0).await?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn next_track(self: &Arc<Self>) -> EngineResult<()> {
        let prefs = self.preferences();
        let step = {
            let pb = self.playback.read();
            next_track_step(pb.queue.queue_index, pb.queue.queue.len(), prefs.repeat_mode)
        };
        match step {
            TrackStep::Play(idx) => self.play_queue_index(idx).await,
            TrackStep::Restart => self.seek_ms(0).await,
            TrackStep::Stop => {
                if let Some(p) = self.player_handle() {
                    p.stop();
                }
                Ok(())
            }
            TrackStep::Ignore => Ok(()),
        }
    }

    pub async fn prev_track(self: &Arc<Self>) -> EngineResult<()> {
        let prefs = self.preferences();
        let step = {
            let pb = self.playback.read();
            previous_track_step(
                pb.queue.position_ms,
                pb.queue.queue_index,
                pb.queue.queue.len(),
                prefs.repeat_mode,
            )
        };
        match step {
            TrackStep::Play(idx) => self.play_queue_index(idx).await,
            TrackStep::Restart => self.seek_ms(0).await,
            TrackStep::Stop | TrackStep::Ignore => Ok(()),
        }
    }

    pub async fn seek_ms(self: &Arc<Self>, position_ms: u64) -> EngineResult<()> {
        let player = self.player_handle().ok_or(EngineError::NotAuthenticated)?;
        let clamped = {
            let mut pb = self.playback.write();
            pb.queue.begin_seek(position_ms, Instant::now());
            pb.queue.position_ms
        };
        self.activity.lock().notify_seek(clamped);
        self.atomic_position_ms.store(clamped, Ordering::Relaxed);
        player.seek(clamped);
        Ok(())
    }

    pub async fn set_volume(self: &Arc<Self>, volume: f32) -> EngineResult<()> {
        let clamped = volume.clamp(0.0, 1.0);
        {
            let mut prefs = self.preferences.write();
            prefs.volume = clamped;
            prefs.bit_perfect_lock = false;
        }
        if clamped > 0.005 {
            self.playback.write().muted_prev_volume = None;
        }
        set_realtime_volume(clamped, false);
        if let Some(player) = self.player_handle() {
            player.set_volume(clamped, false);
        }
        self.schedule_persist_snapshot();
        self.emit(EngineEvent::PreferencesUpdated {
            preferences: self.preferences(),
        });
        Ok(())
    }

    pub async fn toggle_mute(self: &Arc<Self>) -> EngineResult<()> {
        let current = self.preferences().volume;
        let next = {
            let mut pb = self.playback.write();
            if current > 0.005 {
                pb.muted_prev_volume = Some(current);
                0.0
            } else {
                pb.muted_prev_volume.take().unwrap_or(0.8)
            }
        };
        self.set_volume(next).await
    }

    pub async fn set_shuffle(self: &Arc<Self>, enabled: bool) -> EngineResult<()> {
        {
            let mut prefs = self.preferences.write();
            prefs.shuffle_enabled = enabled;
        }
        let (queue, queue_index) = {
            let mut pb = self.playback.write();
            pb.queue.toggle_shuffle(enabled);
            (pb.queue.queue.clone(), pb.queue.queue_index.map(|i| i as u32))
        };
        if let Some(player) = self.player_handle() {
            self.sync_gapless_and_prefetch(&player, &self.preferences());
        }
        self.persist_snapshot();
        self.emit(EngineEvent::QueueUpdated { queue, queue_index });
        self.emit(EngineEvent::PreferencesUpdated {
            preferences: self.preferences(),
        });
        Ok(())
    }

    pub async fn cycle_repeat(self: &Arc<Self>) -> EngineResult<RepeatMode> {
        let next = {
            let mut prefs = self.preferences.write();
            prefs.repeat_mode = prefs.repeat_mode.next();
            prefs.repeat_mode
        };
        if let Some(player) = self.player_handle() {
            self.sync_gapless_and_prefetch(&player, &self.preferences());
        }
        self.persist_snapshot();
        self.emit(EngineEvent::PreferencesUpdated {
            preferences: self.preferences(),
        });
        Ok(next)
    }

    pub async fn queue_play_next(self: &Arc<Self>, track: TrackSummary) -> EngineResult<()> {
        let (queue, queue_index) = {
            let mut pb = self.playback.write();
            pb.queue.insert_next(track);
            (pb.queue.queue.clone(), pb.queue.queue_index.map(|i| i as u32))
        };
        if let Some(player) = self.player_handle() {
            self.sync_gapless_and_prefetch(&player, &self.preferences());
        }
        self.emit(EngineEvent::QueueUpdated { queue, queue_index });
        Ok(())
    }

    pub async fn queue_add(self: &Arc<Self>, track: TrackSummary) -> EngineResult<()> {
        let (queue, queue_index) = {
            let mut pb = self.playback.write();
            pb.queue.append(track);
            (pb.queue.queue.clone(), pb.queue.queue_index.map(|i| i as u32))
        };
        if let Some(player) = self.player_handle() {
            self.sync_gapless_and_prefetch(&player, &self.preferences());
        }
        self.emit(EngineEvent::QueueUpdated { queue, queue_index });
        Ok(())
    }

    pub async fn queue_remove(self: &Arc<Self>, index: usize) -> EngineResult<()> {
        let (changed, queue, queue_index) = {
            let mut pb = self.playback.write();
            let ok = pb.queue.remove_at(index);
            (ok, pb.queue.queue.clone(), pb.queue.queue_index.map(|i| i as u32))
        };
        if changed {
            if let Some(player) = self.player_handle() {
                self.sync_gapless_and_prefetch(&player, &self.preferences());
            }
            self.emit(EngineEvent::QueueUpdated { queue, queue_index });
        }
        Ok(())
    }

    pub async fn queue_move(self: &Arc<Self>, from_idx: usize, to_idx: usize) -> EngineResult<()> {
        let (changed, queue, queue_index) = {
            let mut pb = self.playback.write();
            let ok = pb.queue.move_item(from_idx, to_idx);
            (ok, pb.queue.queue.clone(), pb.queue.queue_index.map(|i| i as u32))
        };
        if changed {
            if let Some(player) = self.player_handle() {
                self.sync_gapless_and_prefetch(&player, &self.preferences());
            }
            self.emit(EngineEvent::QueueUpdated { queue, queue_index });
        }
        Ok(())
    }

    pub async fn queue_clear(self: &Arc<Self>) -> EngineResult<()> {
        let (queue, queue_index) = {
            let mut pb = self.playback.write();
            pb.queue.clear_upcoming();
            (pb.queue.queue.clone(), pb.queue.queue_index.map(|i| i as u32))
        };
        if let Some(player) = self.player_handle() {
            self.sync_gapless_and_prefetch(&player, &self.preferences());
        }
        self.emit(EngineEvent::QueueUpdated { queue, queue_index });
        Ok(())
    }

    pub async fn play_track_radio(self: &Arc<Self>, track_id: u64) -> EngineResult<()> {
        let api = self.api_handle()?;
        let tracks = api
            .track_radio(track_id)
            .await
            .map_err(|e| EngineError::Api(e.to_string()))?;
        if tracks.is_empty() {
            return Err(EngineError::Api("aucune piste radio trouvée".into()));
        }
        self.play_tracks(tracks, 0).await
    }

    pub(crate) async fn on_track_finished(self: &Arc<Self>, finished_track_id: u64) {
        self.flush_active_listening_progress(true);
        let prefs = self.preferences();
        let (step, generation, current_id, current_artist_id) = {
            let pb = self.playback.read();
            let cur = pb.queue.current_track.as_ref();
            if cur.map(|t| t.id) != Some(finished_track_id) {
                return;
            }
            (
                track_finished_step(pb.queue.queue_index, pb.queue.queue.len(), prefs.repeat_mode),
                pb.queue.generation(),
                Some(finished_track_id),
                cur.and_then(|t| t.artist_id),
            )
        };

        match step {
            TrackStep::Play(idx) => {
                let _ = self.play_queue_index(idx).await;
            }
            TrackStep::Restart => {
                let idx = self.playback.read().queue.queue_index;
                if let Some(idx) = idx {
                    let _ = self.play_queue_index(idx).await;
                }
            }
            TrackStep::Stop => {
                if prefs.autoplay_radio {
                    if let (Ok(api), Some(tid)) = (self.api_handle(), current_id) {
                        let mut radio_tracks = api.track_radio(tid).await.unwrap_or_default();
                        if radio_tracks.is_empty() {
                            if let Some(aid) = current_artist_id {
                                radio_tracks = api.artist_radio(aid).await.unwrap_or_default();
                            }
                        }
                        if !radio_tracks.is_empty() {
                            let filtered = filter_tracks(
                                radio_tracks,
                                prefs.allow_explicit,
                                prefs.allow_ai_content,
                            );
                            let next_idx = {
                                let mut pb = self.playback.write();
                                let prev_len = pb.queue.queue.len();
                                if pb.queue.accept_radio_tracks(generation, filtered)
                                    && pb.queue.queue.len() > prev_len
                                {
                                    Some((
                                        prev_len,
                                        pb.queue.queue.clone(),
                                        pb.queue.queue_index.map(|i| i as u32),
                                    ))
                                } else {
                                    None
                                }
                            };
                            if let Some((idx, queue, q_idx)) = next_idx {
                                self.emit(EngineEvent::QueueUpdated {
                                    queue,
                                    queue_index: q_idx,
                                });
                                let _ = self.play_queue_index(idx).await;
                                return;
                            }
                        }
                    }
                }
                self.apply_playback_state(PlaybackState::Stopped);
                self.emit(EngineEvent::PlaybackStateChanged {
                    state: PlaybackState::Stopped,
                });
            }
            TrackStep::Ignore => {}
        }
    }

    pub(crate) async fn on_track_transitioned(
        self: &Arc<Self>,
        _from_track_id: u64,
        to_track_id: u64,
        duration_ms: u64,
    ) {
        let prefs = self.preferences();
        let next_info = {
            let mut pb = self.playback.write();
            let next_idx = next_track_index_for_transition(
                &pb.queue.queue,
                pb.queue.queue_index,
                prefs.repeat_mode,
                to_track_id,
            );
            if let Some(idx) = next_idx {
                pb.queue.select_index(idx).map(|t| {
                    if duration_ms > 0 {
                        pb.queue.duration_ms = duration_ms;
                    }
                    (t, Some(idx as u32), pb.queue.duration_ms, pb.current_palette.clone())
                })
            } else {
                None
            }
        };

        if let Some((track, q_idx, dur, palette)) = next_info {
            self.update_atomic_position(0, dur);
            if let Some(player) = self.player_handle() {
                self.sync_gapless_and_prefetch(&player, &prefs);
            }
            if let Some(ctrl) = self.smtc.lock().as_ref() {
                ctrl.update_metadata(&track);
            }
            self.flush_active_listening_progress(true);
            self.activity.lock().begin_track_session(track.clone());
            self.emit(EngineEvent::TrackChanged {
                track: Some(track.clone()),
                palette,
                queue_index: q_idx,
                duration_ms: dur,
            });
            if let Some(cover_id) = track.cover_id.clone() {
                let eng = Arc::clone(self);
                let tid = track.id;
                self.rt.spawn(async move {
                    let pal = eng.cover_cache.get_palette(&cover_id).await;
                    let still = {
                        let mut pb = eng.playback.write();
                        if pb.queue.current_track.as_ref().map(|t| t.id) == Some(tid) {
                            pb.current_palette = pal.clone();
                            Some((pb.queue.current_track.clone(), pb.queue.queue_index.map(|i| i as u32), pb.queue.duration_ms))
                        } else {
                            None
                        }
                    };
                    if let Some((cur, idx, d)) = still {
                        eng.emit(EngineEvent::TrackChanged {
                            track: cur,
                            palette: pal,
                            queue_index: idx,
                            duration_ms: d,
                        });
                    }
                });
            }
            self.persist_snapshot();
        }
    }

    pub(crate) fn maybe_scrobble_active_track(
        self: &Arc<Self>,
        track: &TrackSummary,
        position_ms: u64,
        duration_ms: u64,
    ) {
        let prefs = self.preferences();
        if !prefs.lastfm_enabled {
            return;
        }
        let scrobble_job = {
            let mut act = self.activity.lock();
            if act.should_scrobble_now(track.id, position_ms, duration_ms) {
                act.active_track_scrobbled = true;
                let started = act.active_track_started_at.unwrap_or(0) as u64;
                act.lastfm_creds.clone().map(|c| (c, started))
            } else {
                None
            }
        };
        if let Some((creds, ts)) = scrobble_job {
            let http = self.http.clone();
            let track_clone = track.clone();
            self.rt.spawn(async move {
                let _ = tidal_api::lastfm::scrobble_track(&http, &creds, &track_clone, ts).await;
            });
        }
    }

    async fn build_personalized_discover_tracks(
        &self,
        api: &TidalApiClient,
        favorites: &[TrackSummary],
        favorite_artists: &[ArtistSummary],
        user_mixes: &[MixSummary],
        rotation: u64,
    ) -> Vec<TrackSummary> {
        let prefs = self.preferences();
        let recent = self.playback.read().queue.play_history.clone();
        let favorite_ids: std::collections::HashSet<u64> =
            favorites.iter().map(|t| t.id).collect();

        let seed_tracks = pick_diverse_seed_tracks(&recent, favorites, rotation, 3);
        let seed_artists = pick_diverse_seed_artists(favorite_artists, rotation, 2);
        let empty_mix_ids: Vec<String> = user_mixes
            .iter()
            .filter(|m| m.tracks.is_empty())
            .take(2)
            .map(|m| m.id.clone())
            .collect();

        let mut buckets: Vec<Vec<TrackSummary>> = Vec::new();
        for mix in user_mixes {
            if !mix.tracks.is_empty() {
                buckets.push(mix.tracks.clone());
            }
        }

        let mut handles = Vec::new();
        for mix_id in empty_mix_ids {
            let api_c = api.clone();
            handles.push(tokio::spawn(async move {
                api_c.mix_tracks(&mix_id, 20).await.unwrap_or_default()
            }));
        }
        for track_id in seed_tracks {
            let api_c = api.clone();
            handles.push(tokio::spawn(async move {
                api_c.track_radio(track_id).await.unwrap_or_default()
            }));
        }
        for artist_id in seed_artists {
            let api_c = api.clone();
            handles.push(tokio::spawn(async move {
                api_c.artist_radio(artist_id).await.unwrap_or_default()
            }));
        }

        for handle in handles {
            if let Ok(tracks) = handle.await {
                if !tracks.is_empty() {
                    buckets.push(tracks);
                }
            }
        }

        let total_candidates: usize = buckets.iter().map(Vec::len).sum();
        if total_candidates < 24 {
            if let Ok(editorial) = api.hires_featured_tracks(30).await {
                if !editorial.is_empty() {
                    buckets.push(editorial);
                }
            }
        }

        curate_diverse_discover_tracks(
            buckets,
            &favorite_ids,
            prefs.allow_explicit,
            prefs.allow_ai_content,
            36,
            2,
            rotation,
        )
    }

    pub async fn refresh_discover(self: &Arc<Self>) -> EngineResult<Vec<TrackSummary>> {
        let api = self.api_handle()?;
        let (favorites, favorite_artists, user_mixes, rotation) = {
            let mut lib = self.library.write();
            lib.discover_rotation = lib.discover_rotation.wrapping_add(1);
            (
                lib.favorites.clone(),
                lib.favorite_artists.clone(),
                lib.user_mixes.clone(),
                lib.discover_rotation,
            )
        };

        let discover_tracks = self
            .build_personalized_discover_tracks(
                &api,
                &favorites,
                &favorite_artists,
                &user_mixes,
                rotation,
            )
            .await;

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if !discover_tracks.is_empty() {
            let disc_db = discover_tracks.clone();
            self.run_db(move |db| {
                let _ = db.upsert_tracks(&disc_db);
            })
            .await;
        }

        let payload = {
            let mut lib = self.library.write();
            if !discover_tracks.is_empty() {
                lib.discover_tracks = discover_tracks.clone();
                lib.discover_refreshed_at = Some(now_secs);
                lib.discover_renewal_pending = false;
            }
            LibrarySyncPayload {
                favorites: lib.favorites.clone(),
                favorite_ids: lib.favorite_ids.iter().copied().collect(),
                playlists: lib.playlists.clone(),
                playlist_folders: lib.playlist_folders.clone(),
                favorite_albums: lib.favorite_albums.clone(),
                favorite_album_ids: lib.favorite_album_ids.iter().copied().collect(),
                favorite_artists: lib.favorite_artists.clone(),
                favorite_artist_ids: lib.favorite_artist_ids.iter().copied().collect(),
                user_mixes: lib.user_mixes.clone(),
                discover_tracks: lib.discover_tracks.clone(),
            }
        };

        self.persist_snapshot();
        self.emit(EngineEvent::LibrarySynced { payload });
        Ok(discover_tracks)
    }

    pub async fn refresh_library(self: &Arc<Self>) -> EngineResult<LibrarySyncPayload> {
        const DISCOVER_FRESH_SECS: u64 = 6 * 3600;
        let api = self.api_handle()?;
        let (fav_res, pl_res, folders_res, albums_res, artists_res, mixes_res) = tokio::join!(
            api.favorite_tracks(500),
            api.user_playlists(200),
            api.user_playlist_folders(),
            api.favorite_albums(200),
            api.favorite_artists(200),
            api.user_mixes(),
        );

        let favorites = fav_res.unwrap_or_default();
        let playlists = pl_res.unwrap_or_default();
        let playlist_folders = folders_res.unwrap_or_default();
        let favorite_albums = albums_res.unwrap_or_default();
        let favorite_artists = artists_res.unwrap_or_default();
        let user_mixes = mixes_res.unwrap_or_default();

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let (need_discover_refresh, rotation) = {
            let mut lib = self.library.write();
            let is_stale = lib
                .discover_refreshed_at
                .map(|ts| now_secs.saturating_sub(ts) >= DISCOVER_FRESH_SECS)
                .unwrap_or(true);
            let need = lib.discover_renewal_pending || lib.discover_tracks.is_empty() || is_stale;
            if need {
                // Nouvelle rotation à chaque régénération (démarrage, obsolescence ou « Varier »)
                // afin que la sélection d'accueil ne soit jamais identique d'une session à l'autre.
                lib.discover_rotation = lib.discover_rotation.wrapping_add(1);
            }
            (need, lib.discover_rotation)
        };

        let discover_tracks = if need_discover_refresh {
            self.build_personalized_discover_tracks(
                &api,
                &favorites,
                &favorite_artists,
                &user_mixes,
                rotation,
            )
            .await
        } else {
            Vec::new()
        };

        let fav_db = favorites.clone();
        let pl_db = playlists.clone();
        let disc_db = discover_tracks.clone();
        self.run_db(move |db| {
            let _ = db.replace_favorites(&fav_db);
            let _ = db.replace_playlists(&pl_db);
            if !disc_db.is_empty() {
                let _ = db.upsert_tracks(&disc_db);
            }
        })
        .await;

        let payload = {
            let mut lib = self.library.write();
            lib.set_favorites(favorites);
            lib.playlists = playlists;
            lib.playlist_folders = playlist_folders;
            lib.set_favorite_albums(favorite_albums);
            lib.set_favorite_artists(favorite_artists);
            if !user_mixes.is_empty() {
                lib.user_mixes = user_mixes;
            }
            if !discover_tracks.is_empty() {
                lib.discover_tracks = discover_tracks;
                lib.discover_refreshed_at = Some(now_secs);
                lib.discover_renewal_pending = false;
            }
            LibrarySyncPayload {
                favorites: lib.favorites.clone(),
                favorite_ids: lib.favorite_ids.iter().copied().collect(),
                playlists: lib.playlists.clone(),
                playlist_folders: lib.playlist_folders.clone(),
                favorite_albums: lib.favorite_albums.clone(),
                favorite_album_ids: lib.favorite_album_ids.iter().copied().collect(),
                favorite_artists: lib.favorite_artists.clone(),
                favorite_artist_ids: lib.favorite_artist_ids.iter().copied().collect(),
                user_mixes: lib.user_mixes.clone(),
                discover_tracks: lib.discover_tracks.clone(),
            }
        };

        self.persist_snapshot();
        self.emit(EngineEvent::LibrarySynced {
            payload: payload.clone(),
        });
        Ok(payload)
    }

    pub async fn search_hybrid(self: &Arc<Self>, query: String) -> EngineResult<HybridSearchResults> {
        let trimmed = query.trim().to_string();
        let seq = {
            let mut lib = self.library.write();
            lib.record_recent_search(&trimmed);
            lib.next_search_seq()
        };
        if trimmed.is_empty() {
            replace_abort_handle(&self.active_search_hybrid, None);
            return Ok(HybridSearchResults {
                seq,
                query: trimmed,
                local_tracks: Vec::new(),
                remote: SearchResults::default(),
            });
        }

        let engine = Arc::clone(self);
        let query_task = trimmed.clone();
        let task = self.rt.spawn(async move {
            let q_local = query_task.clone();
            let local_fut = engine.run_db(move |db| db.search_local_tracks(&q_local, 40).ok());
            let remote_fut = async {
                match engine.api_handle() {
                    Ok(api) => api.search(&query_task, 30).await.unwrap_or_default(),
                    Err(_) => SearchResults::default(),
                }
            };
            let (local_opt, remote) = tokio::join!(local_fut, remote_fut);
            (local_opt.flatten().unwrap_or_default(), remote)
        });
        replace_abort_handle(&self.active_search_hybrid, Some(task.abort_handle()));

        let Ok((local_tracks, remote)) = task.await else {
            return Ok(HybridSearchResults {
                seq,
                query: trimmed,
                local_tracks: Vec::new(),
                remote: SearchResults::default(),
            });
        };

        let is_current = self.library.read().is_current_search(seq);
        if !is_current {
            return Ok(HybridSearchResults {
                seq,
                query: trimmed,
                local_tracks: Vec::new(),
                remote: SearchResults::default(),
            });
        }

        let prefs = self.preferences();
        let local_tracks = filter_tracks(local_tracks, prefs.allow_explicit, prefs.allow_ai_content);
        let mut remote = remote;
        remote.tracks = filter_tracks(remote.tracks, prefs.allow_explicit, prefs.allow_ai_content);
        if let Some(TopHit::Track(ref t)) = remote.top_hit {
            if filter_tracks(vec![t.clone()], prefs.allow_explicit, prefs.allow_ai_content).is_empty()
            {
                remote.top_hit = None;
            }
        }
        self.persist_snapshot();

        Ok(HybridSearchResults {
            seq,
            query: trimmed,
            local_tracks,
            remote,
        })
    }

    pub async fn search_suggestions(self: &Arc<Self>, query: String) -> EngineResult<SearchSuggestions> {
        let trimmed = query.trim().to_string();
        if trimmed.is_empty() {
            replace_abort_handle(&self.active_search_suggestions, None);
            return Ok(SearchSuggestions::default());
        }
        let api = self.api_handle()?;
        let task = self
            .rt
            .spawn(async move { api.search_suggestions(&trimmed).await });
        replace_abort_handle(&self.active_search_suggestions, Some(task.abort_handle()));

        match task.await {
            Ok(Ok(suggestions)) => Ok(suggestions),
            Ok(Err(e)) => Err(EngineError::Api(e.to_string())),
            Err(_) => Ok(SearchSuggestions::default()),
        }
    }

    pub async fn get_explore_hub(self: &Arc<Self>) -> EngineResult<ExploreHubData> {
        let api = self.api_handle()?;
        let hub = api
            .explore_hub()
            .await
            .map_err(|e| EngineError::Api(e.to_string()))?;
        self.library.write().explore_hub = Some(hub.clone());
        Ok(hub)
    }

    pub async fn get_explore_category(
        self: &Arc<Self>,
        category: String,
    ) -> EngineResult<SearchResults> {
        let api = self.api_handle()?;
        let page_paths: &[&str] = match category.as_str() {
            "Nouveautés" => &["pages/explore_new_music", "pages/new_releases"],
            "Top Titres" => &["pages/explore_top_music", "pages/top_20"],
            "Hi-Res / Master" => &["pages/hires", "pages/master"],
            "Ambiances" => &["pages/moods", "pages/explore_moods"],
            _ => &[],
        };
        api.explore_category(page_paths, &category, 24)
            .await
            .map_err(|e| EngineError::Api(e.to_string()))
    }

    pub async fn get_album(self: &Arc<Self>, album_id: u64) -> EngineResult<AlbumDetailData> {
        let api = self.api_handle()?;
        let (details_res, tracks_res, similar_res) = tokio::join!(
            api.album_details(album_id),
            api.album_tracks(album_id),
            api.similar_albums(album_id),
        );
        let tracks = tracks_res.map_err(|e| EngineError::Api(e.to_string()))?;
        let album = details_res
            .ok()
            .flatten()
            .unwrap_or_else(|| {
                let first = tracks.first();
                AlbumSummary {
                    id: album_id,
                    title: first.map(|t| t.album_title.clone()).unwrap_or_default(),
                    artist_name: first.map(|t| t.artist_name.clone()).unwrap_or_default(),
                    artist_id: first.and_then(|t| t.artist_id),
                    cover_id: first.and_then(|t| t.cover_id.clone()),
                    release_date: None,
                    number_of_tracks: tracks.len() as u32,
                    audio_quality: first.and_then(|t| t.audio_quality.clone()),
                }
            });
        let is_favorite = self.library.read().favorite_album_ids.contains(&album_id);
        let prefs = self.preferences();
        Ok(AlbumDetailData {
            album,
            tracks: filter_tracks(tracks, prefs.allow_explicit, prefs.allow_ai_content),
            is_favorite,
            similar_albums: similar_res.unwrap_or_default(),
        })
    }

    pub async fn get_artist(self: &Arc<Self>, artist_id: u64) -> EngineResult<ArtistDetailData> {
        let api = self.api_handle()?;
        api.artist_detail(artist_id)
            .await
            .map_err(|e| EngineError::Api(e.to_string()))
    }

    pub async fn play_artist_radio(self: &Arc<Self>, artist_id: u64) -> EngineResult<()> {
        let api = self.api_handle()?;
        let tracks = api
            .artist_radio(artist_id)
            .await
            .map_err(|e| EngineError::Api(e.to_string()))?;
        if tracks.is_empty() {
            return Err(EngineError::Api("aucune piste radio trouvée".into()));
        }
        self.play_tracks(tracks, 0).await
    }

    pub async fn play_album_radio(self: &Arc<Self>, album_id: u64) -> EngineResult<()> {
        let api = self.api_handle()?;
        let tracks = api
            .album_radio(album_id)
            .await
            .map_err(|e| EngineError::Api(e.to_string()))?;
        if tracks.is_empty() {
            return Err(EngineError::Api("aucune piste radio trouvée pour cet album".into()));
        }
        self.play_tracks(tracks, 0).await
    }

    pub async fn get_playlist_tracks(
        self: &Arc<Self>,
        playlist_uuid: String,
    ) -> EngineResult<PlaylistDetailData> {
        let api = self.api_handle()?;
        let tracks = api
            .playlist_tracks(&playlist_uuid, 500)
            .await
            .map_err(|e| EngineError::Api(e.to_string()))?;
        let pl_uuid_db = playlist_uuid.clone();
        let tracks_db = tracks.clone();
        self.run_db(move |db| {
            let _ = db.replace_playlist_tracks(&pl_uuid_db, &tracks_db);
        })
        .await;
        let playlist = {
            let lib = self.library.read();
            lib.playlists
                .iter()
                .find(|p| p.uuid == playlist_uuid)
                .cloned()
                .or_else(|| {
                    lib.playlist_folders
                        .iter()
                        .flat_map(|f| f.playlists.iter())
                        .find(|p| p.uuid == playlist_uuid)
                        .cloned()
                })
                .unwrap_or(PlaylistSummary {
                    uuid: playlist_uuid,
                    title: "Playlist".into(),
                    description: None,
                    creator: String::new(),
                    number_of_tracks: tracks.len() as u32,
                    image_id: None,
                })
        };
        let prefs = self.preferences();
        let (filtered, positions) =
            filter_tracks_with_positions(tracks, prefs.allow_explicit, prefs.allow_ai_content);
        Ok(PlaylistDetailData {
            playlist,
            tracks: filtered,
            tidal_indices: positions.into_iter().map(|p| p as u32).collect(),
        })
    }

    pub async fn get_mix_tracks(self: &Arc<Self>, mix_id: String) -> EngineResult<Vec<TrackSummary>> {
        let api = self.api_handle()?;
        let tracks = api
            .mix_tracks(&mix_id, 60)
            .await
            .map_err(|e| EngineError::Api(e.to_string()))?;
        let prefs = self.preferences();
        Ok(filter_tracks(tracks, prefs.allow_explicit, prefs.allow_ai_content))
    }

    pub async fn get_lyrics(self: &Arc<Self>, track_id: u64) -> EngineResult<Option<TrackLyrics>> {
        let api = self.api_handle()?;
        api.track_lyrics(track_id)
            .await
            .map_err(|e| EngineError::Api(e.to_string()))
    }

    pub async fn get_credits(self: &Arc<Self>, track_id: u64) -> EngineResult<TrackCredits> {
        let api = self.api_handle()?;
        api.track_credits(track_id)
            .await
            .map_err(|e| EngineError::Api(e.to_string()))
    }

    pub async fn toggle_favorite_track(self: &Arc<Self>, track: TrackSummary) -> EngineResult<bool> {
        let api = self.api_handle()?;
        let added = {
            let mut lib = self.library.write();
            lib.toggle_favorite_track(&track)
        };
        let res = if added {
            api.add_favorite_track(track.id).await
        } else {
            api.remove_favorite_track(track.id).await
        };
        if let Err(err) = res {
            let mut lib = self.library.write();
            lib.toggle_favorite_track(&track);
            return Err(EngineError::Api(err.to_string()));
        }
        self.persist_snapshot();
        Ok(added)
    }

    pub async fn toggle_favorite_album(
        self: &Arc<Self>,
        album_id: u64,
        album: Option<AlbumSummary>,
    ) -> EngineResult<bool> {
        let api = self.api_handle()?;
        let added = {
            let mut lib = self.library.write();
            lib.toggle_favorite_album(album_id, album.as_ref())
        };
        let res = if added {
            api.add_favorite_album(album_id).await
        } else {
            api.remove_favorite_album(album_id).await
        };
        if let Err(err) = res {
            let mut lib = self.library.write();
            lib.toggle_favorite_album(album_id, album.as_ref());
            return Err(EngineError::Api(err.to_string()));
        }
        self.persist_snapshot();
        Ok(added)
    }

    pub async fn toggle_favorite_artist(
        self: &Arc<Self>,
        artist_id: u64,
        artist: Option<ArtistSummary>,
    ) -> EngineResult<bool> {
        let api = self.api_handle()?;
        let added = {
            let mut lib = self.library.write();
            lib.toggle_favorite_artist(artist_id, artist.as_ref())
        };
        let res = if added {
            api.add_favorite_artist(artist_id).await
        } else {
            api.remove_favorite_artist(artist_id).await
        };
        if let Err(err) = res {
            let mut lib = self.library.write();
            lib.toggle_favorite_artist(artist_id, artist.as_ref());
            return Err(EngineError::Api(err.to_string()));
        }
        self.persist_snapshot();
        Ok(added)
    }

    pub async fn create_playlist(
        self: &Arc<Self>,
        title: String,
        description: String,
    ) -> EngineResult<PlaylistSummary> {
        let api = self.api_handle()?;
        let pl = api
            .create_playlist(title.trim(), description.trim())
            .await
            .map_err(|e| EngineError::Api(e.to_string()))?;
        {
            let mut lib = self.library.write();
            lib.playlists.insert(0, pl.clone());
        }
        self.persist_snapshot();
        Ok(pl)
    }

    pub async fn delete_playlist(self: &Arc<Self>, playlist_uuid: String) -> EngineResult<()> {
        let api = self.api_handle()?;
        api.delete_playlist(&playlist_uuid)
            .await
            .map_err(|e| EngineError::Api(e.to_string()))?;
        {
            let mut lib = self.library.write();
            lib.playlists.retain(|p| p.uuid != playlist_uuid);
        }
        self.persist_snapshot();
        Ok(())
    }

    pub async fn add_track_to_playlist(
        self: &Arc<Self>,
        playlist_uuid: String,
        track_id: u64,
    ) -> EngineResult<()> {
        let api = self.api_handle()?;
        api.add_tracks_to_playlist(&playlist_uuid, &[track_id])
            .await
            .map_err(|e| EngineError::Api(e.to_string()))
    }

    pub async fn remove_track_from_playlist(
        self: &Arc<Self>,
        playlist_uuid: String,
        index: usize,
        track_id: u64,
    ) -> EngineResult<()> {
        let api = self.api_handle()?;
        api.remove_track_from_playlist(&playlist_uuid, index, track_id)
            .await
            .map_err(|e| EngineError::Api(e.to_string()))
    }

    pub async fn update_playlist(
        self: &Arc<Self>,
        playlist_uuid: String,
        title: String,
        description: String,
    ) -> EngineResult<PlaylistSummary> {
        let clean_title = title.trim();
        if clean_title.is_empty() {
            return Err(EngineError::InvalidInput(
                "le titre de la playlist ne peut pas être vide".into(),
            ));
        }
        let clean_desc = description.trim();
        let api = self.api_handle()?;
        api.update_playlist(&playlist_uuid, clean_title, clean_desc)
            .await
            .map_err(|e| EngineError::Api(e.to_string()))?;

        let desc_opt = (!clean_desc.is_empty()).then(|| clean_desc.to_string());
        let updated = {
            let mut lib = self.library.write();
            let mut found = None;
            for p in &mut lib.playlists {
                if p.uuid == playlist_uuid {
                    p.title = clean_title.to_string();
                    p.description = desc_opt.clone();
                    found = Some(p.clone());
                }
            }
            for folder in &mut lib.playlist_folders {
                for p in &mut folder.playlists {
                    if p.uuid == playlist_uuid {
                        p.title = clean_title.to_string();
                        p.description = desc_opt.clone();
                        if found.is_none() {
                            found = Some(p.clone());
                        }
                    }
                }
            }
            found.unwrap_or(PlaylistSummary {
                uuid: playlist_uuid,
                title: clean_title.to_string(),
                description: desc_opt,
                creator: String::new(),
                number_of_tracks: 0,
                image_id: None,
            })
        };
        self.persist_snapshot();
        Ok(updated)
    }

    pub async fn move_playlist_track(
        self: &Arc<Self>,
        playlist_uuid: String,
        from_index: usize,
        to_index: usize,
    ) -> EngineResult<()> {
        let api = self.api_handle()?;
        api.move_playlist_track(&playlist_uuid, from_index, to_index)
            .await
            .map_err(|e| EngineError::Api(e.to_string()))
    }

    pub async fn duplicate_playlist(
        self: &Arc<Self>,
        playlist_uuid: String,
        new_title: Option<String>,
    ) -> EngineResult<PlaylistSummary> {
        let api = self.api_handle()?;
        let detail = self.get_playlist_tracks(playlist_uuid).await?;
        let title = new_title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{} (copie)", detail.playlist.title));
        let desc = detail.playlist.description.unwrap_or_default();

        let mut created = api
            .create_playlist(&title, &desc)
            .await
            .map_err(|e| EngineError::Api(e.to_string()))?;

        let track_ids: Vec<u64> = detail.tracks.iter().map(|t| t.id).collect();
        for chunk in track_ids.chunks(50) {
            api.add_tracks_to_playlist(&created.uuid, chunk)
                .await
                .map_err(|e| EngineError::Api(e.to_string()))?;
        }
        created.number_of_tracks = track_ids.len() as u32;
        if created.image_id.is_none() {
            created.image_id = detail.tracks.first().and_then(|t| t.cover_id.clone());
        }
        {
            let mut lib = self.library.write();
            lib.playlists.insert(0, created.clone());
        }
        self.persist_snapshot();
        Ok(created)
    }

    pub async fn export_playlist(
        self: &Arc<Self>,
        playlist_uuid: String,
        format: PlaylistExportFormat,
    ) -> EngineResult<PlaylistExportPayload> {
        let detail = self.get_playlist_tracks(playlist_uuid).await?;
        Ok(playlist_io::export_playlist(
            &detail.playlist.title,
            &detail.tracks,
            format,
        ))
    }

    pub async fn import_playlist(
        self: &Arc<Self>,
        title: String,
        content: String,
        format: PlaylistExportFormat,
    ) -> EngineResult<PlaylistImportResult> {
        let clean_title = title.trim();
        if clean_title.is_empty() {
            return Err(EngineError::InvalidInput(
                "le titre de la playlist importée ne peut pas être vide".into(),
            ));
        }
        let entries = playlist_io::parse_playlist_import(&content, format);
        if entries.is_empty() {
            return Err(EngineError::InvalidInput(
                "aucune piste valide détectée dans le fichier".into(),
            ));
        }

        let api = self.api_handle()?;
        let sem = Arc::new(tokio::sync::Semaphore::new(4));
        let mut tasks = Vec::with_capacity(entries.len());

        for entry in entries {
            let api_ref = Arc::clone(&api);
            let sem_ref = Arc::clone(&sem);
            tasks.push(tokio::spawn(async move {
                if let Some(id) = entry.track_id {
                    return Ok::<u64, String>(id);
                }
                let _permit = sem_ref.acquire().await.ok();
                match api_ref.search(&entry.query, 5).await {
                    Ok(res) => res
                        .tracks
                        .first()
                        .map(|t| t.id)
                        .ok_or_else(|| entry.label.clone()),
                    Err(_) => Err(entry.label.clone()),
                }
            }));
        }

        let mut matched_ids = Vec::new();
        let mut unmatched = Vec::new();
        for t in tasks {
            match t.await {
                Ok(Ok(id)) => matched_ids.push(id),
                Ok(Err(label)) => unmatched.push(label),
                Err(_) => {}
            }
        }

        if matched_ids.is_empty() {
            return Err(EngineError::Api(
                "aucune piste du fichier n'a pu être associée dans le catalogue TIDAL".into(),
            ));
        }

        let mut created = api
            .create_playlist(clean_title, "Importée via TideNative")
            .await
            .map_err(|e| EngineError::Api(e.to_string()))?;

        for chunk in matched_ids.chunks(50) {
            api.add_tracks_to_playlist(&created.uuid, chunk)
                .await
                .map_err(|e| EngineError::Api(e.to_string()))?;
        }
        created.number_of_tracks = matched_ids.len() as u32;
        {
            let mut lib = self.library.write();
            lib.playlists.insert(0, created.clone());
        }
        self.persist_snapshot();

        Ok(PlaylistImportResult {
            playlist: created,
            imported_count: matched_ids.len() as u32,
            unmatched,
        })
    }

    pub async fn update_preferences(self: &Arc<Self>, next: AppPreferences) -> EngineResult<()> {
        let prev = self.preferences();
        {
            let mut guard = self.preferences.write();
            *guard = next.clone();
        }

        set_realtime_volume(next.volume, next.bit_perfect_lock);

        if let Some(player) = self.player_handle() {
            if prev.device_id != next.device_id || prev.exclusive_mode != next.exclusive_mode {
                player.select_device(next.device_id.clone(), next.exclusive_mode);
            }
            if (prev.volume - next.volume).abs() > f32::EPSILON
                || prev.bit_perfect_lock != next.bit_perfect_lock
            {
                player.set_volume(next.volume, next.bit_perfect_lock);
            }
            if prev.normalize_volume != next.normalize_volume {
                player.set_normalization(next.normalize_volume);
            }
            if prev.replay_gain_mode != next.replay_gain_mode {
                player.set_replay_gain_mode(next.replay_gain_mode);
            }
            if prev.gapless_playback != next.gapless_playback {
                player.set_gapless(next.gapless_playback);
                self.sync_gapless_and_prefetch(&player, &next);
            } else if prev.repeat_mode != next.repeat_mode {
                self.sync_gapless_and_prefetch(&player, &next);
            }
        }

        self.persist_snapshot();
        self.emit(EngineEvent::PreferencesUpdated { preferences: next });
        Ok(())
    }

    pub async fn refresh_devices(self: &Arc<Self>) -> EngineResult<Vec<AudioDeviceDescriptor>> {
        let devices =
            enumerate_output_devices().map_err(|e| EngineError::Audio(e.to_string()))?;
        self.set_devices(devices.clone());
        self.emit(EngineEvent::DevicesEnumerated {
            devices: devices.clone(),
        });
        Ok(devices)
    }

    pub async fn connect_lastfm(
        self: &Arc<Self>,
        api_key: String,
        api_secret: String,
        username: String,
        password: String,
    ) -> EngineResult<LastFmStatus> {
        let creds = tidal_api::lastfm::authenticate_mobile(
            &self.http,
            api_key.trim(),
            api_secret.trim(),
            username.trim(),
            &password,
        )
        .await
        .map_err(|e| EngineError::Api(e.to_string()))?;

        let status = {
            let mut act = self.activity.lock();
            act.save_lastfm_creds(creds);
            act.status(true)
        };
        {
            let mut prefs = self.preferences.write();
            prefs.lastfm_enabled = true;
        }
        self.persist_snapshot();
        self.emit(EngineEvent::PreferencesUpdated {
            preferences: self.preferences(),
        });
        Ok(status)
    }

    pub async fn disconnect_lastfm(self: &Arc<Self>) -> EngineResult<LastFmStatus> {
        let status = {
            let mut act = self.activity.lock();
            act.clear_lastfm_creds();
            act.status(false)
        };
        {
            let mut prefs = self.preferences.write();
            prefs.lastfm_enabled = false;
        }
        self.persist_snapshot();
        self.emit(EngineEvent::PreferencesUpdated {
            preferences: self.preferences(),
        });
        Ok(status)
    }

    pub async fn get_activity_stats(
        self: &Arc<Self>,
        period: ActivityPeriod,
    ) -> EngineResult<ListeningActivityStats> {
        self.flush_active_listening_progress_async(true).await;
        let days = period.window_days();
        match self.run_db(move |db| db.load_listening_stats(days, 15)).await {
            Some(res) => res.map_err(|e| EngineError::Cache(e.to_string())),
            None => Ok(ListeningActivityStats::default()),
        }
    }

    pub async fn clear_activity_history(self: &Arc<Self>) -> EngineResult<()> {
        self.activity.lock().reset_active_persistence();
        if let Some(res) = self.run_db(|db| db.clear_listening_history()).await {
            res.map_err(|e| EngineError::Cache(e.to_string()))?;
        }
        Ok(())
    }
}

fn replace_abort_handle(
    slot: &Mutex<Option<tokio::task::AbortHandle>>,
    next: Option<tokio::task::AbortHandle>,
) {
    let prev = {
        let mut guard = slot.lock();
        std::mem::replace(&mut *guard, next)
    };
    if let Some(handle) = prev {
        handle.abort();
    }
}

#[cfg(test)]
mod resilience_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    #[tokio::test]
    async fn replace_abort_handle_cancels_superseded_search_task() {
        let slot = Mutex::new(None);
        let completed = Arc::new(AtomicBool::new(false));
        let completed_flag = Arc::clone(&completed);

        let first = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            completed_flag.store(true, Ordering::Release);
        });
        replace_abort_handle(&slot, Some(first.abort_handle()));

        let second = tokio::spawn(async move { 42u32 });
        replace_abort_handle(&slot, Some(second.abort_handle()));

        let first_res = first.await;
        assert!(first_res.is_err() && first_res.unwrap_err().is_cancelled());
        assert_eq!(second.await.unwrap(), 42);
        assert!(!completed.load(Ordering::Acquire));
    }
}
