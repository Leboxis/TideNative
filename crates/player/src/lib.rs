//! Coordinateur de lecture Bit-Perfect (`tidal-player`).
//!
//! Relie de bout en bout :
//! 1. `tidal-auth` (`/v1/tracks/{id}/playbackinfo` + cache plans + préchargement piste suivante)
//! 2. `tidal-stream` (MPEG-DASH Hi-Res parallèle + cache segments ou BTS HTTP Range -> [`ProgressiveBuffer`])
//! 3. `tidal-decoder` (Symphonia FLAC/fMP4 -> échantillons `i32` alignés MSB)
//! 4. `rtrb` (Ring buffer SPSC lock-free de 2,5 secondes, amorçage rapide 80 ms)
//! 5. `tidal-audio-wasapi` (Sortie matérielle `AUDCLNT_SHAREMODE_EXCLUSIVE` Event-Driven + MMCSS `"Pro Audio"`).
//! 6. Enchaînement **gapless** : la piste suivante est résolue et décodée pendant que le ring
//!    buffer se vide, sans redémarrer le flux WASAPI lorsque le format PCM est identique.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tidal_audio_wasapi::{
    AudioDeviceWatcher, PcmProducer, WasapiActiveStream, WasapiError, WasapiStreamConfig,
    create_pcm_ring_buffer, enumerate_output_devices,
};
use tidal_auth::client::TidalClient;
use tidal_core::{
    AudioQuality, PcmFormat, PlaybackState, PlayerCommand, PlayerEvent, ReplayGainMode,
    StreamFormatInfo,
};
use tidal_decoder::{DecoderError, StreamDecoder};
use tidal_stream::{DashTrackCache, ProgressiveBuffer, StreamDownloader, StreamError, StreamPlan};
use tokio::sync::mpsc;

const PLAN_CACHE_TTL: Duration = Duration::from_secs(12 * 60);
const PLAN_CACHE_MAX_ENTRIES: usize = 24;
const PREFILL_DURATION_MS: u64 = 80;
/// Budget réseau maximal pour préparer la piste suivante pendant que le ring se vide.
const GAPLESS_FETCH_TIMEOUT: Duration = Duration::from_millis(2_500);
/// En dessous de ce seuil, il n'y a plus assez d'audio en avance pour un enchaînement
/// réellement sans coupure : on laisse le chemin `TrackFinished` reprendre la main.
const GAPLESS_MIN_QUEUED_MS: u64 = 50;
/// Période de scrutation des DAC USB / casques (hot-plug Windows Core Audio).
const DEVICE_WATCH_INTERVAL: Duration = Duration::from_secs(2);

/// Encodage compact du mode ReplayGain partagé avec le coordinateur.
const REPLAY_GAIN_OFF: u8 = 0;
const REPLAY_GAIN_TRACK: u8 = 1;
const REPLAY_GAIN_ALBUM: u8 = 2;

const fn replay_gain_mode_to_u8(mode: ReplayGainMode) -> u8 {
    match mode {
        ReplayGainMode::Off => REPLAY_GAIN_OFF,
        ReplayGainMode::Track => REPLAY_GAIN_TRACK,
        ReplayGainMode::Album => REPLAY_GAIN_ALBUM,
    }
}

fn replay_gain_mode_from_u8(raw: u8) -> ReplayGainMode {
    match raw {
        REPLAY_GAIN_OFF => ReplayGainMode::Off,
        REPLAY_GAIN_ALBUM => ReplayGainMode::Album,
        _ => ReplayGainMode::Track,
    }
}

/// Mode résultant de l'ancien réglage booléen `SetNormalization` : désactiver
/// force `Off`, activer réactive `Track` uniquement si rien n'était choisi et
/// ne dégrade jamais un mode `Album` déjà sélectionné.
fn normalization_toggle_mode(current: ReplayGainMode, enabled: bool) -> ReplayGainMode {
    match (enabled, current) {
        (false, _) => ReplayGainMode::Off,
        (true, ReplayGainMode::Off) => ReplayGainMode::Track,
        (true, current) => current,
    }
}

/// Choisit le couple (gain, pic) ReplayGain à appliquer au moteur WASAPI.
///
/// Le troisième booléen indique si la normalisation est active (`false` pour
/// `Off`, ce qui remet le gain à l'unité). En mode `Album`, les valeurs album
/// priment avec repli sur celles de la piste lorsque `PlaybackInfo` ne fournit
/// pas de gain album.
pub fn resolve_replay_gain(
    mode: ReplayGainMode,
    track_gain_db: Option<f32>,
    track_peak: Option<f32>,
    album_gain_db: Option<f32>,
    album_peak: Option<f32>,
) -> (Option<f32>, Option<f32>, bool) {
    match mode {
        ReplayGainMode::Off => (None, None, false),
        ReplayGainMode::Track => (track_gain_db, track_peak, true),
        ReplayGainMode::Album => (
            album_gain_db.or(track_gain_db),
            album_peak.or(track_peak),
            true,
        ),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
    #[error("erreur d'authentification TIDAL : {0}")]
    Auth(#[from] tidal_auth::AuthError),

    #[error("erreur de flux réseau TIDAL : {0}")]
    Stream(#[from] StreamError),

    #[error("erreur de décodage audio : {0}")]
    Decoder(#[from] DecoderError),

    #[error("erreur de sortie audio WASAPI : {0}")]
    Wasapi(#[from] WasapiError),
}

pub type Result<T> = std::result::Result<T, PlayerError>;

/// Entrée en cache pour un plan de lecture (`playbackinfo` + `DashTrackCache`).
#[derive(Clone)]
struct CachedPlan {
    plan: StreamPlan,
    track_replay_gain_db: Option<f32>,
    track_peak_amplitude: Option<f32>,
    album_replay_gain_db: Option<f32>,
    album_peak_amplitude: Option<f32>,
    dash_cache: DashTrackCache,
    fetched_at: Instant,
}

#[derive(Clone, Default)]
struct PlanCache {
    inner: Arc<Mutex<HashMap<(u64, AudioQuality), CachedPlan>>>,
}

impl PlanCache {
    fn get(&self, track_id: u64, quality: AudioQuality) -> Option<CachedPlan> {
        let mut g = self.inner.lock().ok()?;
        let entry = g.get(&(track_id, quality))?;
        if entry.fetched_at.elapsed() > PLAN_CACHE_TTL {
            g.remove(&(track_id, quality));
            return None;
        }
        Some(entry.clone())
    }

    fn insert(&self, track_id: u64, quality: AudioQuality, entry: CachedPlan) {
        if let Ok(mut g) = self.inner.lock() {
            if g.len() >= PLAN_CACHE_MAX_ENTRIES && !g.contains_key(&(track_id, quality)) {
                if let Some(&oldest_key) = g
                    .iter()
                    .min_by_key(|(_, v)| v.fetched_at)
                    .map(|(k, _)| k)
                {
                    g.remove(&oldest_key);
                }
            }
            g.insert((track_id, quality), entry);
        }
    }
}

/// Poignée de contrôle non-bloquante utilisée par l'UI (`iced`) et par SMTC.
#[derive(Clone)]
pub struct PlayerHandle {
    cmd_tx: mpsc::UnboundedSender<PlayerCommand>,
}

impl PlayerHandle {
    pub fn send(&self, cmd: PlayerCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn load_track(&self, track_id: u64, quality: AudioQuality) {
        self.send(PlayerCommand::LoadTrack { track_id, quality });
    }

    pub fn prefetch_track(&self, track_id: u64, quality: AudioQuality) {
        self.send(PlayerCommand::PrefetchTrack { track_id, quality });
    }

    pub fn play(&self) {
        self.send(PlayerCommand::Play);
    }

    pub fn pause(&self) {
        self.send(PlayerCommand::Pause);
    }

    pub fn stop(&self) {
        self.send(PlayerCommand::Stop);
    }

    pub fn seek(&self, position_ms: u64) {
        self.send(PlayerCommand::Seek { position_ms });
    }

    pub fn select_device(&self, device_id: Option<String>, exclusive: bool) {
        self.send(PlayerCommand::SelectDevice {
            device_id,
            exclusive,
        });
    }

    pub fn set_volume(&self, linear: f32, bit_perfect_lock: bool) {
        tidal_audio_wasapi::set_realtime_volume(linear, bit_perfect_lock);
        self.send(PlayerCommand::SetVolume {
            linear,
            bit_perfect_lock,
        });
    }

    pub fn set_normalization(&self, enabled: bool) {
        self.send(PlayerCommand::SetNormalization { enabled });
    }

    /// Sélectionne le mode ReplayGain (Désactivé, Piste ou Album).
    pub fn set_replay_gain_mode(&self, mode: ReplayGainMode) {
        self.send(PlayerCommand::SetReplayGainMode { mode });
    }

    /// Prépare la piste suivante pour un enchaînement sans coupure (gapless).
    /// `None` annule l'enchaînement en attente.
    pub fn queue_gapless_next(&self, track_id: Option<u64>, quality: AudioQuality) {
        self.send(PlayerCommand::QueueGaplessNext { track_id, quality });
    }

    /// Active ou désactive l'enchaînement gapless (actif par défaut).
    pub fn set_gapless(&self, enabled: bool) {
        self.send(PlayerCommand::SetGapless { enabled });
    }
}

/// Piste réellement en cours de lecture (mise à jour lors d'un enchaînement gapless).
#[derive(Debug, Clone, Copy)]
struct ActiveTrackState {
    track_id: u64,
    quality: AudioQuality,
    track_replay_gain_db: Option<f32>,
    track_peak_amplitude: Option<f32>,
    album_replay_gain_db: Option<f32>,
    album_peak_amplitude: Option<f32>,
}

/// Tampon réseau réellement en cours de lecture au sein d'une session.
///
/// Lors d'un enchaînement gapless (ou d'un seek DASH), le thread décodeur remplace
/// son tampon : le slot partagé suit ce remplacement pour que `Stop`/`LoadTrack`
/// annule immédiatement la connexion de la piste enchaînée, et pas seulement
/// celle du démarrage de la session.
#[derive(Clone)]
struct CurrentBufferSlot {
    inner: Arc<Mutex<ProgressiveBuffer>>,
}

impl CurrentBufferSlot {
    fn new(buffer: ProgressiveBuffer) -> Self {
        Self {
            inner: Arc::new(Mutex::new(buffer)),
        }
    }

    fn replace(&self, buffer: ProgressiveBuffer) {
        if let Ok(mut current) = self.inner.lock() {
            *current = buffer;
        }
    }

    fn cancel(&self) {
        // Clone sous verrou puis annule hors verrou : `cancel` prend le mutex
        // interne du tampon, jamais détenu en même temps que ce slot par le
        // thread décodeur.
        let current = self.inner.lock().ok().map(|guard| guard.clone());
        if let Some(buffer) = current {
            buffer.cancel();
        }
    }
}

/// Annule le tampon courant si la tâche de chargement est abandonnée
/// (`LoadTrack`/`Stop` plus récent) ou échoue avant de rendre la session.
struct BufferCancelGuard {
    slot: CurrentBufferSlot,
    armed: bool,
}

impl Drop for BufferCancelGuard {
    fn drop(&mut self) {
        if self.armed {
            self.slot.cancel();
        }
    }
}

/// Session de décodage + rendu WASAPI d'une piste active.
struct ActiveTrackSession {
    current: Arc<Mutex<ActiveTrackState>>,
    cancel_flag: Arc<AtomicBool>,
    /// Pause logique partagée avec le thread décodeur : une fin de piste ne doit
    /// pas enchaîner automatiquement tant que l'utilisateur n'a pas repris.
    paused: Arc<AtomicBool>,
    pending_seek_ms: Arc<AtomicI64>,
    buffer: CurrentBufferSlot,
    wasapi: WasapiActiveStream,
}

impl ActiveTrackSession {
    fn stop(&mut self) {
        self.cancel_flag.store(true, Ordering::Release);
        self.buffer.cancel();
        self.wasapi.stop();
    }
}

impl Drop for ActiveTrackSession {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Réapplique immédiatement le ReplayGain au flux WASAPI actif, sans attendre
/// le prochain chargement de piste (changement de mode à chaud).
fn apply_replay_gain_for_mode(active: Option<&ActiveTrackSession>, mode: ReplayGainMode) {
    let resolved = active.and_then(|session| {
        session.current.lock().ok().map(|state| {
            resolve_replay_gain(
                mode,
                state.track_replay_gain_db,
                state.track_peak_amplitude,
                state.album_replay_gain_db,
                state.album_peak_amplitude,
            )
        })
    });
    let (gain_db, peak_amplitude, enabled) =
        resolved.unwrap_or_else(|| resolve_replay_gain(mode, None, None, None, None));
    tidal_audio_wasapi::set_replay_gain(gain_db, peak_amplitude, enabled);
}

/// Démarre l'acteur principal du lecteur audio en arrière-plan et renvoie
/// `(PlayerHandle, Receiver<PlayerEvent>)`.
pub fn spawn_player_actor(
    client: Arc<TidalClient>,
    initial_config: WasapiStreamConfig,
) -> (PlayerHandle, mpsc::UnboundedReceiver<PlayerEvent>) {
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<PlayerCommand>();
    let (evt_tx, evt_rx) = mpsc::unbounded_channel::<PlayerEvent>();

    let gain_q24 = Arc::new(AtomicU32::new(1 << 24));
    let bit_perfect_lock = Arc::new(AtomicBool::new(true));
    let replay_gain_mode = Arc::new(AtomicU8::new(REPLAY_GAIN_TRACK));

    tokio::spawn(async move {
        if let Ok(devs) = enumerate_output_devices() {
            let _ = evt_tx.send(PlayerEvent::DevicesEnumerated(devs));
        }

        // Hot-plug USB DAC / casque : le watcher vit aussi longtemps que la tâche
        // du lecteur et est joint proprement à sa sortie.
        let hotplug_evt_tx = evt_tx.clone();
        let _device_watcher = AudioDeviceWatcher::start(DEVICE_WATCH_INTERVAL, move |devices| {
            let _ = hotplug_evt_tx.send(PlayerEvent::DevicesEnumerated(devices));
        });

        // Partage le même pool HTTP/2 + TLS keep-alive que TidalClient
        let downloader = StreamDownloader::with_client(client.http());
        let plan_cache = PlanCache::default();
        let gapless_enabled = Arc::new(AtomicBool::new(true));
        let gapless_next: Arc<Mutex<Option<(u64, AudioQuality)>>> = Arc::new(Mutex::new(None));

        let mut wasapi_cfg = initial_config;
        let mut active: Option<ActiveTrackSession> = None;
        let mut load_gen: u64 = 0;
        let mut pending_load: Option<tokio::task::JoinHandle<()>> = None;
        let mut pending_prefetch: Option<tokio::task::JoinHandle<()>> = None;
        let (loaded_tx, mut loaded_rx) =
            mpsc::unbounded_channel::<(u64, Result<ActiveTrackSession>)>();

        loop {
            tokio::select! {
                biased;

                Some((generation, outcome)) = loaded_rx.recv() => {
                    if generation != load_gen {
                        // Une commande plus récente a remplacé ce chargement : libère proprement
                        if let Ok(mut stale) = outcome {
                            stale.stop();
                        }
                        continue;
                    }
                    pending_load = None;
                    match outcome {
                        Ok(session) => {
                            // Le mode a pu changer pendant le chargement réseau :
                            // on applique le mode courant au moment où la piste
                            // devient réellement active.
                            apply_replay_gain_for_mode(
                                Some(&session),
                                replay_gain_mode_from_u8(
                                    replay_gain_mode.load(Ordering::Acquire),
                                ),
                            );
                            let mut info = session.wasapi.stream_format_info().clone();
                            let lock = bit_perfect_lock.load(Ordering::Acquire);
                            let q24 = gain_q24.load(Ordering::Acquire);
                            info.is_bit_perfect =
                                info.is_bit_perfect && (lock || q24 >= (1 << 24));
                            let _ = evt_tx.send(PlayerEvent::FormatNegotiated(info));
                            let _ = evt_tx.send(PlayerEvent::StateChanged(PlaybackState::Playing));
                            active = Some(session);
                        }
                        Err(e) => {
                            let _ = evt_tx.send(PlayerEvent::StateChanged(
                                PlaybackState::Error(e.to_string()),
                            ));
                        }
                    }
                }

                maybe_cmd = cmd_rx.recv() => {
                    let Some(cmd) = maybe_cmd else {
                        break;
                    };
                    match cmd {
                        PlayerCommand::SelectDevice {
                            device_id,
                            exclusive,
                        } => {
                            wasapi_cfg.device_id = device_id;
                            wasapi_cfg.exclusive = exclusive;
                            if let Ok(devs) = enumerate_output_devices() {
                                let _ = evt_tx.send(PlayerEvent::DevicesEnumerated(devs));
                            }
                            if let Some(mut prev) = active.take() {
                                let current = prev.current.lock().ok().map(|guard| *guard);
                                prev.stop();
                                load_gen = load_gen.wrapping_add(1);
                                if let Some(h) = pending_load.take() {
                                    h.abort();
                                }
                                if let Some(state) = current {
                                    let _ = evt_tx
                                        .send(PlayerEvent::StateChanged(PlaybackState::Buffering));
                                    pending_load = Some(spawn_load_task(
                                        load_gen,
                                        Arc::clone(&client),
                                        downloader.clone(),
                                        plan_cache.clone(),
                                        state.track_id,
                                        state.quality,
                                        wasapi_cfg.clone(),
                                        Arc::clone(&gain_q24),
                                        Arc::clone(&bit_perfect_lock),
                                        Arc::clone(&replay_gain_mode),
                                        Arc::clone(&gapless_enabled),
                                        Arc::clone(&gapless_next),
                                        evt_tx.clone(),
                                        loaded_tx.clone(),
                                    ));
                                }
                            }
                        }
                        PlayerCommand::SetVolume {
                            linear,
                            bit_perfect_lock: lock,
                        } => {
                            tidal_audio_wasapi::set_realtime_volume(linear, lock);
                            let clamped = linear.clamp(0.0, 1.0);
                            let q24 = ((f64::from(clamped) * f64::from(1u32 << 24)).round() as u32)
                                .min(1 << 24);

                            if let Some(sess) = &active {
                                let mut info = sess.wasapi.stream_format_info().clone();
                                info.is_bit_perfect =
                                    info.is_bit_perfect && (lock || q24 >= (1 << 24));
                                let _ = evt_tx.send(PlayerEvent::FormatNegotiated(info));
                            }
                        }
                        PlayerCommand::SetNormalization { enabled } => {
                            // Compatibilité : l'ancien réglage booléen pilote le
                            // mode ReplayGain sans jamais dégrader un choix Album.
                            let mode = normalization_toggle_mode(
                                replay_gain_mode_from_u8(
                                    replay_gain_mode.load(Ordering::Acquire),
                                ),
                                enabled,
                            );
                            replay_gain_mode.store(replay_gain_mode_to_u8(mode), Ordering::Release);
                            apply_replay_gain_for_mode(active.as_ref(), mode);
                        }
                        PlayerCommand::SetReplayGainMode { mode } => {
                            replay_gain_mode.store(replay_gain_mode_to_u8(mode), Ordering::Release);
                            // Réapplique immédiatement le couple (gain, pic) de la piste
                            // réellement en cours, y compris après un enchaînement gapless.
                            apply_replay_gain_for_mode(active.as_ref(), mode);
                        }
                        PlayerCommand::Pause => {
                            if let Some(sess) = &active {
                                sess.paused.store(true, Ordering::Release);
                                sess.wasapi.pause();
                                let _ = evt_tx.send(PlayerEvent::StateChanged(PlaybackState::Paused));
                            }
                        }
                        PlayerCommand::Play => {
                            if let Some(sess) = &active {
                                sess.paused.store(false, Ordering::Release);
                                sess.wasapi.resume();
                                let _ = evt_tx.send(PlayerEvent::StateChanged(PlaybackState::Playing));
                            }
                        }
                        PlayerCommand::Stop => {
                            load_gen = load_gen.wrapping_add(1);
                            if let Some(h) = pending_load.take() {
                                h.abort();
                            }
                            if let Some(mut sess) = active.take() {
                                sess.stop();
                            }
                            // Une entrée gapless appartient à la session arrêtée.
                            if let Ok(mut slot) = gapless_next.lock() {
                                *slot = None;
                            }
                            let _ = evt_tx.send(PlayerEvent::StateChanged(PlaybackState::Stopped));
                        }
                        PlayerCommand::Seek { position_ms } => {
                            if let Some(sess) = &active {
                                sess.pending_seek_ms
                                    .store(position_ms as i64, Ordering::Release);
                            }
                        }
                        PlayerCommand::PrefetchTrack { track_id, quality } => {
                            if plan_cache.get(track_id, quality).is_some() {
                                continue;
                            }
                            if let Some(prev) = pending_prefetch.take() {
                                prev.abort();
                            }
                            let pf_client = Arc::clone(&client);
                            let pf_dl = downloader.clone();
                            let pf_cache = plan_cache.clone();
                            pending_prefetch = Some(tokio::spawn(async move {
                                // Laisse la priorité absolue à la piste qui vient de démarrer
                                tokio::time::sleep(Duration::from_millis(450)).await;
                                if let Ok(cached) =
                                    resolve_or_fetch_plan(&pf_client, &pf_cache, track_id, quality)
                                        .await
                                {
                                    if let StreamPlan::Dash(dash) = &cached.plan {
                                        pf_dl.prefetch_dash_start(dash, &cached.dash_cache).await;
                                    }
                                }
                            }));
                        }
                        PlayerCommand::SetGapless { enabled } => {
                            gapless_enabled.store(enabled, Ordering::Release);
                        }
                        PlayerCommand::QueueGaplessNext { track_id, quality } => {
                            if let Ok(mut slot) = gapless_next.lock() {
                                *slot = track_id.map(|id| (id, quality));
                            }
                            // Préchauffe immédiatement `init.mp4` + 1er segment en RAM :
                            // le thread décodeur les retrouvera dans `PlanCache` à l'EOF.
                            if let Some(id) = track_id {
                                if plan_cache.get(id, quality).is_none() {
                                    if let Some(prev) = pending_prefetch.take() {
                                        prev.abort();
                                    }
                                    let pf_client = Arc::clone(&client);
                                    let pf_dl = downloader.clone();
                                    let pf_cache = plan_cache.clone();
                                    pending_prefetch = Some(tokio::spawn(async move {
                                        if let Ok(cached) = resolve_or_fetch_plan(
                                            &pf_client, &pf_cache, id, quality,
                                        )
                                        .await
                                        {
                                            if let StreamPlan::Dash(dash) = &cached.plan {
                                                pf_dl
                                                    .prefetch_dash_start(dash, &cached.dash_cache)
                                                    .await;
                                            }
                                        }
                                    }));
                                }
                            }
                        }
                        PlayerCommand::LoadTrack { track_id, quality } => {
                            load_gen = load_gen.wrapping_add(1);
                            if let Some(h) = pending_load.take() {
                                h.abort();
                            }
                            if let Some(mut prev) = active.take() {
                                prev.stop();
                            }
                            // La piste suivante de l'ancienne file ne vaut plus pour la nouvelle :
                            // l'UI renvoie `QueueGaplessNext` après ce `LoadTrack`.
                            if let Ok(mut slot) = gapless_next.lock() {
                                *slot = None;
                            }
                            let _ = evt_tx.send(PlayerEvent::StateChanged(PlaybackState::Buffering));

                            pending_load = Some(spawn_load_task(
                                load_gen,
                                Arc::clone(&client),
                                downloader.clone(),
                                plan_cache.clone(),
                                track_id,
                                quality,
                                wasapi_cfg.clone(),
                                Arc::clone(&gain_q24),
                                Arc::clone(&bit_perfect_lock),
                                Arc::clone(&replay_gain_mode),
                                Arc::clone(&gapless_enabled),
                                Arc::clone(&gapless_next),
                                evt_tx.clone(),
                                loaded_tx.clone(),
                            ));
                        }
                    }
                }
            }
        }

        if let Some(h) = pending_load.take() {
            h.abort();
        }
        if let Some(h) = pending_prefetch.take() {
            h.abort();
        }
    });

    (PlayerHandle { cmd_tx }, evt_rx)
}

#[allow(clippy::too_many_arguments)]
fn spawn_load_task(
    generation: u64,
    client: Arc<TidalClient>,
    downloader: StreamDownloader,
    plan_cache: PlanCache,
    track_id: u64,
    quality: AudioQuality,
    wasapi_cfg: WasapiStreamConfig,
    gain_q24: Arc<AtomicU32>,
    bit_perfect_lock: Arc<AtomicBool>,
    replay_gain_mode: Arc<AtomicU8>,
    gapless_enabled: Arc<AtomicBool>,
    gapless_next: Arc<Mutex<Option<(u64, AudioQuality)>>>,
    evt_tx: mpsc::UnboundedSender<PlayerEvent>,
    loaded_tx: mpsc::UnboundedSender<(u64, Result<ActiveTrackSession>)>,
) -> tokio::task::JoinHandle<()> {
    let worker_client = Arc::clone(&client);
    tokio::spawn(async move {
        let res = start_track_pipeline(
            &client,
            &downloader,
            &plan_cache,
            track_id,
            quality,
            wasapi_cfg,
            gain_q24,
            bit_perfect_lock,
            replay_gain_mode,
            Some(worker_client),
            gapless_enabled,
            gapless_next,
            evt_tx,
        )
        .await;
        let _ = loaded_tx.send((generation, res));
    })
}

async fn resolve_or_fetch_plan(
    client: &TidalClient,
    plan_cache: &PlanCache,
    track_id: u64,
    quality: AudioQuality,
) -> Result<CachedPlan> {
    if let Some(hit) = plan_cache.get(track_id, quality) {
        return Ok(hit);
    }
    let pb_info = client.playback_info(track_id, quality.as_api_str()).await?;
    let plan = StreamPlan::from_playback_info(&pb_info)?;
    let entry = CachedPlan {
        plan,
        track_replay_gain_db: pb_info.track_replay_gain,
        track_peak_amplitude: pb_info.track_peak_amplitude,
        album_replay_gain_db: pb_info.album_replay_gain,
        album_peak_amplitude: pb_info.album_peak_amplitude,
        dash_cache: DashTrackCache::new(),
        fetched_at: Instant::now(),
    };
    plan_cache.insert(track_id, quality, entry.clone());
    Ok(entry)
}

#[allow(clippy::too_many_arguments)]
async fn start_track_pipeline(
    client: &TidalClient,
    downloader: &StreamDownloader,
    plan_cache: &PlanCache,
    track_id: u64,
    quality: AudioQuality,
    wasapi_cfg: WasapiStreamConfig,
    gain_q24: Arc<AtomicU32>,
    bit_perfect_lock: Arc<AtomicBool>,
    replay_gain_mode: Arc<AtomicU8>,
    worker_client: Option<Arc<TidalClient>>,
    gapless_enabled: Arc<AtomicBool>,
    gapless_next: Arc<Mutex<Option<(u64, AudioQuality)>>>,
    evt_tx: mpsc::UnboundedSender<PlayerEvent>,
) -> Result<ActiveTrackSession> {
    let cached = resolve_or_fetch_plan(client, plan_cache, track_id, quality).await?;
    let track_replay_gain_db = cached.track_replay_gain_db;
    let track_peak_amplitude = cached.track_peak_amplitude;
    let album_replay_gain_db = cached.album_replay_gain_db;
    let album_peak_amplitude = cached.album_peak_amplitude;
    let (gain_db, peak_amplitude, replay_gain_enabled) = resolve_replay_gain(
        replay_gain_mode_from_u8(replay_gain_mode.load(Ordering::Acquire)),
        track_replay_gain_db,
        track_peak_amplitude,
        album_replay_gain_db,
        album_peak_amplitude,
    );
    tidal_audio_wasapi::set_replay_gain(gain_db, peak_amplitude, replay_gain_enabled);

    let mut plan = cached.plan;
    let dash_cache = cached.dash_cache;
    let is_dash = matches!(plan, StreamPlan::Dash(_));
    let mut mime_hint = plan.mime_type().to_string();
    let dash_duration_ms = match &plan {
        StreamPlan::Dash(d) => Some(d.total_duration_ms()),
        StreamPlan::Direct { .. } => None,
    };

    // Démarre dès init.mp4 + 64 Ko du 1er segment (DASH) ou 48 Ko (Direct/BTS)
    let buffer = downloader
        .start_progressive_cached(plan.clone(), &dash_cache)
        .await?;
    let buffer_slot = CurrentBufferSlot::new(buffer.clone());
    let mut cancel_guard = BufferCancelGuard {
        slot: buffer_slot.clone(),
        armed: true,
    };

    let mut decoder = StreamDecoder::open(&buffer, Some(&mime_hint), is_dash)?;
    let pcm_format = decoder.pcm_format();
    let codec_label = decoder.codec_label().to_string();
    let mut total_duration_ms = decoder.duration_ms().or(dash_duration_ms).unwrap_or(0);

    // Ring buffer SPSC lock-free de 2 500 ms (~3.8 Mo en 24-bit / 192 kHz)
    let (mut producer, consumer) = create_pcm_ring_buffer(pcm_format, 2_500);
    let ring_capacity_samples = producer.slots();
    let mut prng_state: u64 = 0x9E37_79B9_7F4A_7C15 ^ track_id;

    // Pré-décodage d'amorçage ultra-rapide (80 ms) avant le démarrage de l'horloge WASAPI
    let prefill_samples =
        pcm_format.frames_for_duration(Duration::from_millis(PREFILL_DURATION_MS))
            * pcm_format.samples_per_frame();
    let mut decode_scratch = Vec::with_capacity(16_384);

    while producer.slots() > 0 {
        decode_scratch.clear();
        match decoder.decode_next(&mut decode_scratch)? {
            Some(_) => {
                apply_optional_gain_tpdf(
                    &mut decode_scratch,
                    &gain_q24,
                    &bit_perfect_lock,
                    &mut prng_state,
                );
                push_samples_blocking(
                    &mut producer,
                    &decode_scratch,
                    &AtomicBool::new(false),
                    None,
                );
                let filled = ring_capacity_samples.saturating_sub(producer.slots());
                if filled >= prefill_samples {
                    break;
                }
            }
            None => break,
        }
    }

    tidal_audio_wasapi::set_replay_gain(gain_db, peak_amplitude, replay_gain_enabled);
    let wasapi = WasapiActiveStream::start(wasapi_cfg, pcm_format, codec_label, consumer)?;
    let dac_clock = wasapi.clock();
    let flush_ring = wasapi.flush_ring_handle();

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_worker = Arc::clone(&cancel_flag);
    let paused_flag = Arc::new(AtomicBool::new(false));
    let thread_paused = Arc::clone(&paused_flag);
    let pending_seek_ms = Arc::new(AtomicI64::new(-1));
    let seek_worker = Arc::clone(&pending_seek_ms);
    let active_buffer = buffer.clone();
    let thread_buffer_slot = buffer_slot.clone();
    let worker_downloader = downloader.clone();
    let mut worker_dash_cache = dash_cache.clone();
    let worker_plan_cache = plan_cache.clone();
    let thread_replay_gain_mode = Arc::clone(&replay_gain_mode);
    let shared_current = Arc::new(Mutex::new(ActiveTrackState {
        track_id,
        quality,
        track_replay_gain_db,
        track_peak_amplitude,
        album_replay_gain_db,
        album_peak_amplitude,
    }));
    let thread_current = Arc::clone(&shared_current);
    let rt_handle = tokio::runtime::Handle::current();
    let thread_name = format!("tidenative-decoder-{track_id}");
    // Le thread décodeur réassigne la piste active lors d'un enchaînement gapless.
    let mut track_id = track_id;

    thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            let mut pkt_buf = Vec::with_capacity(16_384);
            let mut last_report = Instant::now();
            let mut seek_offset_ms: i64 = 0;
            let channels = pcm_format.samples_per_frame().max(1) as u64;
            let mut current_buffer = active_buffer;

            while !cancel_worker.load(Ordering::Acquire) {
                let req_seek = seek_worker.swap(-1, Ordering::AcqRel);
                if req_seek >= 0 {
                    // Coupe immédiatement l'ancien audio en attente dans le ring buffer dès le clic
                    flush_ring.store(true, Ordering::Release);

                    let target_ms = (req_seek as u64).min(total_duration_ms.max(req_seek as u64));
                    let seek_result: Option<u64> = match &plan {
                        StreamPlan::Dash(dash) => {
                            let seg_idx = dash.segment_index_at_ms(target_ms);
                            let seg_start_ms = if dash.timescale > 0 {
                                dash.segments
                                    .get(seg_idx)
                                    .map(|s| (s.start_ticks * 1000) / dash.timescale)
                                    .unwrap_or(0)
                            } else {
                                0
                            };
                            current_buffer.cancel();
                            match rt_handle.block_on(
                                worker_downloader.start_dash_from_segment_cached(
                                    dash.clone(),
                                    seg_idx,
                                    &worker_dash_cache,
                                ),
                            ) {
                                Ok(new_buf) => {
                                    // Si un autre seek plus récent est déjà arrivé, saute immédiatement au suivant
                                    if seek_worker.load(Ordering::Acquire) >= 0 {
                                        new_buf.cancel();
                                        continue;
                                    }
                                    match StreamDecoder::open(&new_buf, Some(&mime_hint), true) {
                                        Ok(mut new_dec) => {
                                            let delta_ms = target_ms.saturating_sub(seg_start_ms);
                                            let mut skip_frames = ((delta_ms as u128
                                                * u128::from(pcm_format.sample_rate))
                                                / 1000)
                                                as u64;
                                            let mut skipped_frames = 0u64;
                                            let mut superseded = false;
                                            while skip_frames > 0
                                                && !cancel_worker.load(Ordering::Acquire)
                                            {
                                                if seek_worker.load(Ordering::Acquire) >= 0 {
                                                    superseded = true;
                                                    break;
                                                }
                                                pkt_buf.clear();
                                                match new_dec.decode_next(&mut pkt_buf) {
                                                    Ok(Some(frames)) => {
                                                        let f = frames as u64;
                                                        skipped_frames += f;
                                                        skip_frames = skip_frames.saturating_sub(f);
                                                    }
                                                    _ => break,
                                                }
                                            }
                                            if superseded {
                                                new_buf.cancel();
                                                continue;
                                            }
                                            thread_buffer_slot.replace(new_buf.clone());
                                            current_buffer = new_buf;
                                            decoder = new_dec;
                                            let actual_ms = seg_start_ms
                                                + pcm_format.frames_to_ms(skipped_frames);
                                            Some(actual_ms)
                                        }
                                        Err(_) => {
                                            new_buf.cancel();
                                            None
                                        }
                                    }
                                }
                                Err(_) => None,
                            }
                        }
                        StreamPlan::Direct { .. } => decoder.seek_ms(target_ms).ok(),
                    };

                    if let Some(actual_ms) = seek_result {
                        flush_ring.store(true, Ordering::Release);
                        let flush_deadline = Instant::now() + Duration::from_millis(25);
                        while flush_ring.load(Ordering::Acquire)
                            && !cancel_worker.load(Ordering::Acquire)
                            && Instant::now() < flush_deadline
                        {
                            thread::sleep(Duration::from_millis(1));
                        }
                        let rendered_ms = dac_clock.rendered_position_ms() as i64;
                        seek_offset_ms = (actual_ms as i64) - rendered_ms;
                        let _ = evt_tx.send(PlayerEvent::PositionUpdated {
                            position_ms: actual_ms,
                            duration_ms: total_duration_ms,
                        });
                        last_report = Instant::now();
                    }
                }

                pkt_buf.clear();
                match decoder.decode_next(&mut pkt_buf) {
                    Ok(Some(_frames)) => {
                        apply_optional_gain_tpdf(
                            &mut pkt_buf,
                            &gain_q24,
                            &bit_perfect_lock,
                            &mut prng_state,
                        );
                        if !push_samples_blocking(
                            &mut producer,
                            &pkt_buf,
                            &cancel_worker,
                            Some(&seek_worker),
                        ) {
                            break;
                        }

                        if last_report.elapsed() >= Duration::from_millis(120) {
                            let pos_ms = compute_playback_position_ms(
                                dac_clock.rendered_position_ms(),
                                seek_offset_ms,
                                total_duration_ms,
                            );
                            let _ = evt_tx.send(PlayerEvent::PositionUpdated {
                                position_ms: pos_ms,
                                duration_ms: total_duration_ms,
                            });
                            let queued_samples =
                                ring_capacity_samples.saturating_sub(producer.slots()) as u64;
                            let queued_frames = queued_samples / channels;
                            let _ = evt_tx.send(PlayerEvent::BufferHealth {
                                buffered_ms: pcm_format.frames_to_ms(queued_frames),
                                underruns: dac_clock.underruns(),
                            });
                            last_report = Instant::now();
                        }
                    }
                    Ok(None) => {
                        // Consomme l'entrée gapless une seule fois, même si l'enchaînement
                        // échoue : une entrée périmée ne doit jamais déclencher de transition
                        // fantôme lors d'une session ultérieure.
                        let queued_next = gapless_next.lock().ok().and_then(|mut slot| slot.take());
                        if let Some((next_id, next_quality)) = queued_next {
                            if gapless_enabled.load(Ordering::Acquire)
                                && !cancel_worker.load(Ordering::Acquire)
                                && seek_worker.load(Ordering::Acquire) < 0
                            {
                                let queued_samples =
                                    ring_capacity_samples.saturating_sub(producer.slots()) as u64;
                                let queued_ms = pcm_format.frames_to_ms(queued_samples / channels);
                                if queued_ms >= GAPLESS_MIN_QUEUED_MS {
                                    if let Some(worker_client) = worker_client.as_ref() {
                                        if let Some(transition) = prepare_gapless_transition(
                                            &rt_handle,
                                            worker_client,
                                            &worker_downloader,
                                            &worker_plan_cache,
                                            track_id,
                                            next_id,
                                            next_quality,
                                            pcm_format,
                                            &cancel_worker,
                                            &seek_worker,
                                        ) {
                                            // Le ring buffer a continué de se vider vers le DAC
                                            // pendant la préparation réseau : on mesure les trames
                                            // encore en avance pour caler l'horloge de la piste
                                            // suivante sur la fin exacte de la précédente.
                                            let queued_samples = ring_capacity_samples
                                                .saturating_sub(producer.slots())
                                                as u64;
                                            let queued_frames = queued_samples / channels;
                                            let boundary_rendered_ms = dac_clock
                                                .rendered_position_ms()
                                                .saturating_add(pcm_format.frames_to_ms(
                                                    queued_frames,
                                                ));
                                            seek_offset_ms = -(boundary_rendered_ms as i64);
                                            let next_track_gain =
                                                transition.track_replay_gain_db;
                                            let next_track_peak =
                                                transition.track_peak_amplitude;
                                            let next_album_gain =
                                                transition.album_replay_gain_db;
                                            let next_album_peak =
                                                transition.album_peak_amplitude;
                                            let (next_gain, next_peak, rg_enabled) =
                                                resolve_replay_gain(
                                                    replay_gain_mode_from_u8(
                                                        thread_replay_gain_mode
                                                            .load(Ordering::Acquire),
                                                    ),
                                                    next_track_gain,
                                                    next_track_peak,
                                                    next_album_gain,
                                                    next_album_peak,
                                                );
                                            tidal_audio_wasapi::set_replay_gain(
                                                next_gain,
                                                next_peak,
                                                rg_enabled,
                                            );
                                            let from_track_id = transition.from_track_id;
                                            let to_track_id = transition.to_track_id;
                                            plan = transition.plan;
                                            worker_dash_cache = transition.dash_cache;
                                            mime_hint = transition.mime_hint;
                                            thread_buffer_slot.replace(transition.buffer.clone());
                                            current_buffer = transition.buffer;
                                            decoder = transition.decoder;
                                            total_duration_ms = transition.duration_ms;
                                            track_id = to_track_id;
                                            prng_state = 0x9E37_79B9_7F4A_7C15 ^ to_track_id;
                                            if let Ok(mut current) = thread_current.lock() {
                                                *current = ActiveTrackState {
                                                    track_id: to_track_id,
                                                    quality: next_quality,
                                                    track_replay_gain_db: next_track_gain,
                                                    track_peak_amplitude: next_track_peak,
                                                    album_replay_gain_db: next_album_gain,
                                                    album_peak_amplitude: next_album_peak,
                                                };
                                            }
                                            let _ = evt_tx.send(
                                                PlayerEvent::TrackTransitioned {
                                                    from_track_id,
                                                    to_track_id,
                                                    duration_ms: total_duration_ms,
                                                },
                                            );
                                            last_report = Instant::now();
                                            // Le premier échantillon PCM de la piste suivante
                                            // est poussé immédiatement après le dernier de la
                                            // précédente : aucune coupure, aucun redémarrage WASAPI.
                                            continue;
                                        }
                                    }
                                }
                            }
                        }

                        let mut drain_deadline = Instant::now() + Duration::from_secs(4);
                        let mut reseeked = false;
                        while !cancel_worker.load(Ordering::Acquire)
                            && Instant::now() < drain_deadline
                        {
                            if seek_worker.load(Ordering::Acquire) >= 0 {
                                reseeked = true;
                                break;
                            }
                            if thread_paused.load(Ordering::Acquire) {
                                // Fin de piste pendant une pause : attendre la reprise
                                // plutôt que de lancer le titre suivant tout seul.
                                drain_deadline = Instant::now() + Duration::from_secs(4);
                                thread::sleep(Duration::from_millis(40));
                                continue;
                            }
                            if producer.slots() >= ring_capacity_samples {
                                break;
                            }
                            let pos_ms = compute_playback_position_ms(
                                dac_clock.rendered_position_ms(),
                                seek_offset_ms,
                                total_duration_ms,
                            );
                            let _ = evt_tx.send(PlayerEvent::PositionUpdated {
                                position_ms: pos_ms,
                                duration_ms: total_duration_ms,
                            });
                            thread::sleep(Duration::from_millis(40));
                        }
                        if reseeked {
                            continue;
                        }
                        if !cancel_worker.load(Ordering::Acquire) {
                            let _ = evt_tx.send(PlayerEvent::TrackFinished { track_id });
                        }
                        break;
                    }
                    Err(e) => {
                        // Une annulation de session fait échouer la lecture du flux :
                        // ce n'est pas une erreur utilisateur, on sort silencieusement.
                        if !cancel_worker.load(Ordering::Acquire) {
                            let _ = evt_tx.send(PlayerEvent::StateChanged(
                                PlaybackState::Error(e.to_string()),
                            ));
                        }
                        break;
                    }
                }
            }

            // Libère la connexion réellement active, y compris celle de la piste
            // enchaînée en gapless après un `Stop`/`LoadTrack` (le slot partagé a
            // pu être remplacé après que la session a été arrêtée).
            current_buffer.cancel();
        })
        .expect("spawn decoder thread");

    cancel_guard.armed = false;
    Ok(ActiveTrackSession {
        current: shared_current,
        cancel_flag,
        paused: paused_flag,
        pending_seek_ms,
        buffer: buffer_slot,
        wasapi,
    })
}

/// Piste suivante décodée et prête à être enchaînée sans redémarrer WASAPI.
struct GaplessTransition {
    from_track_id: u64,
    to_track_id: u64,
    duration_ms: u64,
    track_replay_gain_db: Option<f32>,
    track_peak_amplitude: Option<f32>,
    album_replay_gain_db: Option<f32>,
    album_peak_amplitude: Option<f32>,
    plan: StreamPlan,
    dash_cache: DashTrackCache,
    mime_hint: String,
    buffer: ProgressiveBuffer,
    decoder: StreamDecoder,
}

/// Prépare la piste suivante pendant que le ring buffer de la piste courante se vide :
/// résolution du plan (`PlaybackInfo` + cache), démarrage du [`ProgressiveBuffer`]
/// (init.mp4 + 1er segment déjà chauds) puis ouverture du décodeur.
///
/// Retourne `None` si le format PCM diffère (il faut alors rouvrir un flux WASAPI
/// négocié), si un seek/stop est survenu ou si le réseau dépasse le budget imparti.
#[allow(clippy::too_many_arguments)]
fn prepare_gapless_transition(
    rt_handle: &tokio::runtime::Handle,
    client: &TidalClient,
    downloader: &StreamDownloader,
    plan_cache: &PlanCache,
    current_track_id: u64,
    next_id: u64,
    next_quality: AudioQuality,
    pcm_format: PcmFormat,
    cancel_worker: &AtomicBool,
    seek_worker: &AtomicI64,
) -> Option<GaplessTransition> {
    if cancel_worker.load(Ordering::Acquire) || seek_worker.load(Ordering::Acquire) >= 0 {
        return None;
    }

    let (cached, plan, dash_cache, buffer) = rt_handle.block_on(async {
        tokio::time::timeout(GAPLESS_FETCH_TIMEOUT, async {
            let cached = resolve_or_fetch_plan(client, plan_cache, next_id, next_quality)
                .await
                .ok()?;
            let plan = cached.plan.clone();
            let dash_cache = cached.dash_cache.clone();
            let buffer = downloader
                .start_progressive_cached(plan.clone(), &dash_cache)
                .await
                .ok()?;
            Some((cached, plan, dash_cache, buffer))
        })
        .await
        .ok()
        .flatten()
    })?;

    if cancel_worker.load(Ordering::Acquire) || seek_worker.load(Ordering::Acquire) >= 0 {
        buffer.cancel();
        return None;
    }

    let is_dash = matches!(plan, StreamPlan::Dash(_));
    let mime_hint = plan.mime_type().to_string();
    let dash_duration_ms = match &plan {
        StreamPlan::Dash(dash) => Some(dash.total_duration_ms()),
        StreamPlan::Direct { .. } => None,
    };
    let decoder = match StreamDecoder::open(&buffer, Some(&mime_hint), is_dash) {
        Ok(decoder) => decoder,
        Err(_) => {
            buffer.cancel();
            return None;
        }
    };
    if decoder.pcm_format() != pcm_format {
        buffer.cancel();
        return None;
    }
    let duration_ms = decoder.duration_ms().or(dash_duration_ms).unwrap_or(0);

    Some(GaplessTransition {
        from_track_id: current_track_id,
        to_track_id: next_id,
        duration_ms,
        track_replay_gain_db: cached.track_replay_gain_db,
        track_peak_amplitude: cached.track_peak_amplitude,
        album_replay_gain_db: cached.album_replay_gain_db,
        album_peak_amplitude: cached.album_peak_amplitude,
        plan,
        dash_cache,
        mime_hint,
        buffer,
        decoder,
    })
}

/// Lit un titre TIDAL de bout en bout (utilisé par le binaire CLI `tidal-player`).
pub async fn play_track_blocking(
    client: &TidalClient,
    track_id: u64,
    quality: AudioQuality,
    wasapi_cfg: WasapiStreamConfig,
    on_format: impl FnOnce(&StreamFormatInfo),
) -> Result<()> {
    let downloader = StreamDownloader::with_client(client.http());
    let plan_cache = PlanCache::default();
    let gain_q24 = Arc::new(AtomicU32::new(1 << 24));
    let bit_perfect_lock = Arc::new(AtomicBool::new(true));
    let (evt_tx, mut evt_rx) = mpsc::unbounded_channel();

    let session = start_track_pipeline(
        client,
        &downloader,
        &plan_cache,
        track_id,
        quality,
        wasapi_cfg,
        gain_q24,
        bit_perfect_lock,
        Arc::new(AtomicU8::new(REPLAY_GAIN_TRACK)),
        None,
        Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(None)),
        evt_tx,
    )
    .await?;

    on_format(session.wasapi.stream_format_info());

    while let Some(evt) = evt_rx.recv().await {
        match evt {
            PlayerEvent::TrackFinished { .. } => {
                tokio::time::sleep(Duration::from_millis(80)).await;
                break;
            }
            PlayerEvent::StateChanged(PlaybackState::Error(err)) => {
                return Err(PlayerError::Wasapi(WasapiError::Initialization(err)));
            }
            _ => {}
        }
    }

    Ok(())
}

/// Calcule la position de lecture affichée à partir de l'horloge matérielle du DAC et du décalage seek.
#[inline]
pub fn compute_playback_position_ms(
    dac_rendered_ms: u64,
    seek_offset_ms: i64,
    total_duration_ms: u64,
) -> u64 {
    let pos = (dac_rendered_ms as i64).saturating_add(seek_offset_ms).max(0) as u64;
    if total_duration_ms > 0 {
        pos.min(total_duration_ms)
    } else {
        pos
    }
}

/// Applique le gain logiciel en virgule fixe Q24 avec **dithering TPDF 24-bit**
/// (Triangular Probability Density Function) uniquement si le verrou Bit-Perfect est désactivé.
/// Lorsque `bit_perfect_lock == true` ou `gain == 1.0`, **aucun bit n'est modifié**.
#[inline]
pub fn apply_optional_gain_tpdf(
    samples: &mut [i32],
    gain_q24: &AtomicU32,
    bit_perfect_lock: &AtomicBool,
    prng_state: &mut u64,
) {
    if bit_perfect_lock.load(Ordering::Acquire) {
        return;
    }
    let q24 = i64::from(gain_q24.load(Ordering::Acquire));
    if q24 >= (1 << 24) {
        return;
    }
    for s in samples.iter_mut() {
        let scaled = (i64::from(*s) * q24) >> 24;
        let r1 = xorshift64(prng_state) & 0xFF;
        let r2 = xorshift64(prng_state) & 0xFF;
        let dither = (r1 as i64) - (r2 as i64);
        *s = (scaled + dither).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    }
}

#[inline]
fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn push_samples_blocking(
    producer: &mut PcmProducer,
    samples: &[i32],
    cancel: &AtomicBool,
    pending_seek: Option<&AtomicI64>,
) -> bool {
    let mut offset = 0;
    while offset < samples.len() {
        if cancel.load(Ordering::Acquire) {
            return false;
        }
        if let Some(seek) = pending_seek {
            if seek.load(Ordering::Acquire) >= 0 {
                return true;
            }
        }
        let avail = producer.slots();
        if avail == 0 {
            thread::sleep(Duration::from_millis(2));
            continue;
        }
        let chunk_len = (samples.len() - offset).min(avail);
        if let Ok(mut chunk) = producer.write_chunk(chunk_len) {
            let (s1, s2) = chunk.as_mut_slices();
            let l1 = s1.len();
            s1.copy_from_slice(&samples[offset..offset + l1]);
            s2.copy_from_slice(&samples[offset + l1..offset + l1 + s2.len()]);
            chunk.commit_all();
            offset += chunk_len;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_perfect_lock_leaves_samples_bit_identical() {
        let original = vec![0x1234_5600i32, -0x7654_3200, 0, i32::MAX & !0xFF];
        let mut samples = original.clone();
        let gain_q24 = AtomicU32::new(1 << 23);
        let lock = AtomicBool::new(true);
        let mut prng = 42u64;

        apply_optional_gain_tpdf(&mut samples, &gain_q24, &lock, &mut prng);
        assert_eq!(samples, original);
    }

    #[test]
    fn unlocked_volume_attenuates_with_bounded_tpdf_dither() {
        let mut samples = vec![0x4000_0000i32; 8];
        let gain_q24 = AtomicU32::new(1 << 23);
        let lock = AtomicBool::new(false);
        let mut prng = 0xDEAD_BEEF_CAFE_BABEu64;

        apply_optional_gain_tpdf(&mut samples, &gain_q24, &lock, &mut prng);
        for s in samples {
            let diff = (i64::from(s) - 0x2000_0000i64).abs();
            assert!(diff <= 255, "TPDF dither out of 1-LSB 24-bit bound: {diff}");
        }
    }

    #[test]
    fn hardware_clock_position_respects_seek_offset_and_clamps() {
        assert_eq!(compute_playback_position_ms(12_000, 0, 200_000), 12_000);
        assert_eq!(compute_playback_position_ms(15_000, 80_000, 200_000), 95_000);
        assert_eq!(compute_playback_position_ms(40_000, -35_000, 200_000), 5_000);
        assert_eq!(compute_playback_position_ms(250_000, 0, 200_000), 200_000);
    }

    /// Lors d'un enchaînement gapless, `seek_offset_ms = -boundary_rendered_ms` :
    /// la position de la nouvelle piste reste à 0 pendant que le DAC finit de vider
    /// les ~2 s de l'ancienne, puis compte à partir de 0 ms sans à-coup.
    #[test]
    fn gapless_boundary_clock_restarts_at_zero_then_counts_up() {
        let boundary_ms = 182_000u64;
        let seek_offset_ms = -(boundary_ms as i64);
        let next_track_duration_ms = 245_000;

        // Pendant la vidange du ring buffer (horloge encore avant la frontière).
        assert_eq!(
            compute_playback_position_ms(180_250, seek_offset_ms, next_track_duration_ms),
            0
        );
        // À la frontière exacte : début de la piste suivante.
        assert_eq!(
            compute_playback_position_ms(boundary_ms, seek_offset_ms, next_track_duration_ms),
            0
        );
        // Après la frontière : la position repart de 0 ms pour la nouvelle piste.
        assert_eq!(
            compute_playback_position_ms(boundary_ms + 1_250, seek_offset_ms, next_track_duration_ms),
            1_250
        );
        // Fin de la nouvelle piste : plafonnée à sa durée.
        assert_eq!(
            compute_playback_position_ms(
                boundary_ms + 400_000,
                seek_offset_ms,
                next_track_duration_ms
            ),
            next_track_duration_ms
        );
    }

    #[test]
    fn shared_buffer_slot_cancels_the_active_gapless_buffer() {
        let first = ProgressiveBuffer::with_capacity(64, 1);
        let second = ProgressiveBuffer::with_capacity(64, 1);
        let slot = CurrentBufferSlot::new(first.clone());

        slot.replace(second.clone());
        assert!(!second.is_cancelled());

        slot.cancel();
        assert!(second.is_cancelled(), "le tampon gapless courant doit être annulé");
        assert!(
            !first.is_cancelled(),
            "seul le tampon réellement actif est annulé"
        );
    }

    #[test]
    fn buffer_cancel_guard_cancels_pending_load_unless_disarmed() {
        let aborted = ProgressiveBuffer::with_capacity(64, 1);
        {
            let _guard = BufferCancelGuard {
                slot: CurrentBufferSlot::new(aborted.clone()),
                armed: true,
            };
        }
        assert!(aborted.is_cancelled(), "un chargement abandonné libère le réseau");

        let completed = ProgressiveBuffer::with_capacity(64, 1);
        {
            let mut guard = BufferCancelGuard {
                slot: CurrentBufferSlot::new(completed.clone()),
                armed: true,
            };
            guard.armed = false;
        }
        assert!(
            !completed.is_cancelled(),
            "une session rendue à l'acteur garde son tampon actif"
        );
    }

    #[test]
    fn replay_gain_off_disables_normalization() {
        assert_eq!(
            resolve_replay_gain(
                ReplayGainMode::Off,
                Some(-6.0),
                Some(0.9),
                Some(-4.0),
                Some(0.8),
            ),
            (None, None, false)
        );
    }

    #[test]
    fn replay_gain_track_prefers_track_values() {
        assert_eq!(
            resolve_replay_gain(
                ReplayGainMode::Track,
                Some(-6.0),
                Some(0.9),
                Some(-4.0),
                Some(0.8),
            ),
            (Some(-6.0), Some(0.9), true)
        );
    }

    #[test]
    fn replay_gain_album_prefers_album_values() {
        assert_eq!(
            resolve_replay_gain(
                ReplayGainMode::Album,
                Some(-6.0),
                Some(0.9),
                Some(-4.0),
                Some(0.8),
            ),
            (Some(-4.0), Some(0.8), true)
        );
    }

    #[test]
    fn replay_gain_album_falls_back_to_track_when_album_missing() {
        assert_eq!(
            resolve_replay_gain(ReplayGainMode::Album, Some(-6.0), Some(0.9), None, None),
            (Some(-6.0), Some(0.9), true)
        );
        // Repli partiel : un seul des deux champs album est absent.
        assert_eq!(
            resolve_replay_gain(ReplayGainMode::Album, Some(-6.0), Some(0.9), Some(-4.0), None),
            (Some(-4.0), Some(0.9), true)
        );
        // Aucune valeur disponible : normalisation active mais gain unité.
        assert_eq!(
            resolve_replay_gain(ReplayGainMode::Album, None, None, None, None),
            (None, None, true)
        );
    }

    #[test]
    fn replay_gain_mode_u8_roundtrip_falls_back_to_track() {
        for mode in ReplayGainMode::ALL {
            assert_eq!(replay_gain_mode_from_u8(replay_gain_mode_to_u8(mode)), mode);
        }
        assert_eq!(replay_gain_mode_from_u8(255), ReplayGainMode::Track);
    }

    #[test]
    fn normalization_toggle_never_downgrades_album_mode() {
        assert_eq!(
            normalization_toggle_mode(ReplayGainMode::Album, true),
            ReplayGainMode::Album,
            "l'ancien booléen ne doit pas écraser un choix Album explicite"
        );
        assert_eq!(
            normalization_toggle_mode(ReplayGainMode::Off, true),
            ReplayGainMode::Track
        );
        assert_eq!(
            normalization_toggle_mode(ReplayGainMode::Track, true),
            ReplayGainMode::Track
        );
        assert_eq!(
            normalization_toggle_mode(ReplayGainMode::Album, false),
            ReplayGainMode::Off
        );
        assert_eq!(
            normalization_toggle_mode(ReplayGainMode::Off, false),
            ReplayGainMode::Off
        );
    }
}
