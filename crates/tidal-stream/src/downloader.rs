//! Téléchargeur asynchrone HTTP/2 pour flux TIDAL Direct (BTS) et segmentés
//! (MPEG-DASH Hi-Res FLAC).
//!
//! Optimisations faible latence :
//! - **DASH** : `init.mp4` et le 1er segment partent en **parallèle** (`tokio::try_join!`),
//!   et le 1er segment est poussé **progressivement par chunks** dès les premiers 64 Ko
//!   (au lieu d'attendre 2 segments complets ~5 Mo). Les segments déjà téléchargés et
//!   `init.mp4` sont mis en cache dans [`DashTrackCache`] pour un seek instantané.
//! - **Direct/BTS** : démarrage dès 48 Ko + gestionnaire HTTP `Range` à la demande pour
//!   sauter instantanément n'importe où dans un FLAC 16/44.1 sans attendre les Mo intermédiaires.

use crate::buffer::{ProgressiveBuffer, RangeRequest};
use crate::mpd::{DashStreamPlan, parse_mpd};
use crate::{Result, StreamError};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tidal_auth::client::PlaybackInfo;
use tidal_auth::manifest::Manifest;

/// Seuil minimal de pré-remplissage avant d'ouvrir Symphonia sur le 1er segment DASH.
const DASH_FIRST_CHUNK_PRELOAD: usize = 64 * 1024;
/// Seuil minimal de pré-remplissage avant d'ouvrir Symphonia sur un flux Direct/BTS.
const DIRECT_PRELOAD_BYTES: usize = 48 * 1024;
/// Nombre maximal de segments DASH conservés en RAM par piste pour le seek instantané.
const MAX_CACHED_DASH_SEGMENTS: usize = 48;

/// Cache mémoire des segments MPEG-DASH (`init.mp4` + segments média) pour la piste active.
#[derive(Clone, Default)]
pub struct DashTrackCache {
    init: Arc<Mutex<Option<Arc<Vec<u8>>>>>,
    segments: Arc<Mutex<HashMap<usize, Arc<Vec<u8>>>>>,
}

impl DashTrackCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_init(&self) -> Option<Arc<Vec<u8>>> {
        self.init.lock().ok()?.clone()
    }

    pub fn set_init(&self, bytes: Vec<u8>) -> Arc<Vec<u8>> {
        let arc = Arc::new(bytes);
        if let Ok(mut g) = self.init.lock() {
            *g = Some(Arc::clone(&arc));
        }
        arc
    }

    pub fn get_segment(&self, idx: usize) -> Option<Arc<Vec<u8>>> {
        self.segments.lock().ok()?.get(&idx).cloned()
    }

    pub fn set_segment(&self, idx: usize, bytes: Vec<u8>) {
        if let Ok(mut g) = self.segments.lock() {
            if g.len() >= MAX_CACHED_DASH_SEGMENTS && !g.contains_key(&idx) {
                // Évince le segment le plus éloigné de l'index courant
                if let Some(&farthest) = g.keys().max_by_key(|&&k| k.abs_diff(idx)) {
                    g.remove(&farthest);
                }
            }
            g.insert(idx, Arc::new(bytes));
        }
    }
}

/// Plan de streaming unifié prêt à être exécuté par [`StreamDownloader`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamPlan {
    /// Flux continu sur une URL unique (`application/vnd.tidal.bts`, FLAC 16/44.1 ou AAC).
    Direct {
        url: String,
        mime_type: String,
        codecs: String,
    },
    /// Flux segmenté fMP4 (`application/dash+xml`, FLAC Hi-Res jusqu'à 24-bit / 192 kHz).
    Dash(DashStreamPlan),
}

impl StreamPlan {
    /// Construit un [`StreamPlan`] à partir d'un [`Manifest`] décodé par `tidal-auth`.
    pub fn from_manifest(manifest: &Manifest) -> Result<Self> {
        match manifest {
            Manifest::Bts(bts) => {
                let url = bts
                    .urls
                    .first()
                    .cloned()
                    .ok_or_else(|| StreamError::EmptyManifest("BTS sans URL".into()))?;
                Ok(Self::Direct {
                    url,
                    mime_type: bts.mime_type.clone(),
                    codecs: bts.codecs.clone().unwrap_or_else(|| "flac".into()),
                })
            }
            Manifest::Dash { mpd_xml } => {
                let dash = parse_mpd(mpd_xml)?;
                Ok(Self::Dash(dash))
            }
        }
    }

    /// Construit un [`StreamPlan`] directement depuis la réponse `/playbackinfo` de TIDAL.
    pub fn from_playback_info(info: &PlaybackInfo) -> Result<Self> {
        let decoded = Manifest::decode(&info.manifest_mime_type, &info.manifest)?;
        Self::from_manifest(&decoded)
    }

    /// Codec annoncé par le manifest (`"flac"`, `"mp4a.40.2"`, etc.).
    pub fn codecs(&self) -> &str {
        match self {
            Self::Direct { codecs, .. } => codecs,
            Self::Dash(d) => &d.codecs,
        }
    }

    /// Type MIME du conteneur (`"audio/flac"`, `"audio/mp4"`, etc.).
    pub fn mime_type(&self) -> &str {
        match self {
            Self::Direct { mime_type, .. } => mime_type,
            Self::Dash(d) => &d.mime_type,
        }
    }
}

/// Téléchargeur asynchrone alimentant un [`ProgressiveBuffer`].
#[derive(Clone)]
pub struct StreamDownloader {
    http: reqwest::Client,
}

impl StreamDownloader {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: tidal_auth::http_client()?,
        })
    }

    pub fn with_client(http: reqwest::Client) -> Self {
        Self { http }
    }

    /// Lance le téléchargement du [`StreamPlan`] et rend la main dès que les premiers
    /// octets décodables sont en RAM (typiquement 1 seul aller-retour réseau).
    pub async fn start_progressive(
        &self,
        plan: StreamPlan,
        _initial_segments: usize,
    ) -> Result<ProgressiveBuffer> {
        let cache = DashTrackCache::new();
        self.start_progressive_cached(plan, &cache).await
    }

    /// Variante utilisant un [`DashTrackCache`] partagé pour le démarrage et les seeks.
    pub async fn start_progressive_cached(
        &self,
        plan: StreamPlan,
        dash_cache: &DashTrackCache,
    ) -> Result<ProgressiveBuffer> {
        match plan {
            StreamPlan::Dash(dash) => {
                self.start_dash_from_segment_cached(dash, 0, dash_cache)
                    .await
            }
            StreamPlan::Direct { url, .. } => self.start_direct(url).await,
        }
    }

    /// Pré-télécharge `init.mp4` + le 1er segment d'une piste DASH (pour un passage à la
    /// piste suivante quasi instantané).
    pub async fn prefetch_dash_start(&self, dash: &DashStreamPlan, cache: &DashTrackCache) {
        if cache.get_init().is_none() {
            if let Ok(bytes) = fetch_with_retry(&self.http, &dash.init_url, 2).await {
                cache.set_init(bytes);
            }
        }
        if let Some(seg0) = dash.segments.first() {
            if cache.get_segment(0).is_none() {
                if let Ok(bytes) = fetch_with_retry(&self.http, &seg0.url, 2).await {
                    cache.set_segment(0, bytes);
                }
            }
        }
    }

    /// Démarre un flux MPEG-DASH à partir de `start_segment_idx` (sans cache externe).
    pub async fn start_dash_from_segment(
        &self,
        dash: DashStreamPlan,
        start_segment_idx: usize,
        _initial_segments: usize,
    ) -> Result<ProgressiveBuffer> {
        let cache = DashTrackCache::new();
        self.start_dash_from_segment_cached(dash, start_segment_idx, &cache)
            .await
    }

    /// Démarre un flux MPEG-DASH à partir de `start_segment_idx` en réutilisant `cache` :
    /// - Si `init.mp4` et le segment cible sont déjà en RAM -> retour en < 1 ms.
    /// - Sinon, télécharge `init.mp4` (si absent) et le segment cible en **parallèle**,
    ///   et rend la main dès les premiers 64 Ko du segment cible.
    pub async fn start_dash_from_segment_cached(
        &self,
        dash: DashStreamPlan,
        start_segment_idx: usize,
        cache: &DashTrackCache,
    ) -> Result<ProgressiveBuffer> {
        let total_segments = dash.segments.len();
        if total_segments == 0 {
            return Err(StreamError::EmptyManifest("MPD sans segments".into()));
        }
        let start_idx = start_segment_idx.min(total_segments - 1);
        let slice_len = total_segments - start_idx;
        let buffer = ProgressiveBuffer::with_capacity(4 * 1024 * 1024, slice_len.max(1));

        let cached_init = cache.get_init();
        let cached_first = cache.get_segment(start_idx);

        // Cas 1 : init + 1er segment déjà en cache (seek instantané 0 ms réseau !)
        if let (Some(init_bytes), Some(first_seg)) = (cached_init.clone(), cached_first) {
            buffer.push_chunk(&init_bytes, false);
            if !buffer.push_chunk(&first_seg, true) {
                return Ok(buffer);
            }
            spawn_dash_background(
                self.http.clone(),
                dash,
                start_idx + 1,
                buffer.clone(),
                cache.clone(),
                None,
            );
            return Ok(buffer);
        }

        // Cas 2 : lancement parallèle init.mp4 (si pas en cache) + ouverture stream segment cible
        let first_url = dash.segments[start_idx].url.clone();
        let (init_bytes, mut first_resp) = if let Some(init_bytes) = cached_init {
            let resp = send_with_retry(&self.http, &first_url, 3).await?;
            (init_bytes, resp)
        } else {
            let (init_raw, resp) = tokio::try_join!(
                fetch_with_retry(&self.http, &dash.init_url, 3),
                send_with_retry(&self.http, &first_url, 3),
            )?;
            (cache.set_init(init_raw), resp)
        };

        buffer.push_chunk(&init_bytes, false);

        // Streame les premiers 64 Ko du segment cible dans ProgressiveBuffer pour démarrer
        // Symphonia sans attendre les 2-3 Mo complets du segment Hi-Res.
        let mut first_acc = Vec::with_capacity(
            first_resp
                .content_length()
                .map(|l| (l as usize).min(4 * 1024 * 1024))
                .unwrap_or(512 * 1024),
        );
        let mut first_done = false;
        while first_acc.len() < DASH_FIRST_CHUNK_PRELOAD {
            match first_resp.chunk().await? {
                Some(chunk) => {
                    first_acc.extend_from_slice(&chunk);
                    if !buffer.push_chunk(&chunk, false) {
                        return Ok(buffer);
                    }
                }
                None => {
                    first_done = true;
                    break;
                }
            }
        }

        if first_done {
            cache.set_segment(start_idx, first_acc);
            buffer.push_chunk(&[], true);
            if start_idx + 1 >= total_segments {
                buffer.finish();
                return Ok(buffer);
            }
            spawn_dash_background(
                self.http.clone(),
                dash,
                start_idx + 1,
                buffer.clone(),
                cache.clone(),
                None,
            );
        } else {
            spawn_dash_background(
                self.http.clone(),
                dash,
                start_idx + 1,
                buffer.clone(),
                cache.clone(),
                Some((start_idx, first_resp, first_acc)),
            );
        }

        Ok(buffer)
    }

    async fn start_direct(&self, url: String) -> Result<ProgressiveBuffer> {
        let mut resp = send_with_retry(&self.http, &url, 3).await?;
        let content_len = resp.content_length();
        let init_cap = content_len
            .map(|l| (l as usize).min(64 * 1024 * 1024))
            .unwrap_or(4 * 1024 * 1024);

        let buffer = ProgressiveBuffer::with_capacity(init_cap, 1);
        if let Some(len) = content_len {
            buffer.set_total_expected(len);
        }

        // Pré-remplir 48 Ko (suffisant pour en-tête FLAC/MP4 + premières trames)
        while buffer.downloaded_bytes() < DIRECT_PRELOAD_BYTES {
            match resp.chunk().await? {
                Some(chunk) => {
                    if !buffer.push_chunk(&chunk, false) {
                        return Ok(buffer);
                    }
                }
                None => {
                    buffer.finish();
                    return Ok(buffer);
                }
            }
        }

        // Active le worker HTTP Range à la demande si la taille est connue
        if content_len.is_some() {
            let range_rx = buffer.enable_range_seeking();
            spawn_range_worker(self.http.clone(), url, buffer.clone(), range_rx);
        }

        let bg_buffer = buffer.clone();
        tokio::spawn(async move {
            loop {
                if bg_buffer.is_cancelled() || bg_buffer.should_pause_prefix() {
                    return;
                }
                match resp.chunk().await {
                    Ok(Some(chunk)) => {
                        if !bg_buffer.push_chunk(&chunk, false) {
                            return;
                        }
                    }
                    Ok(None) => {
                        bg_buffer.finish();
                        return;
                    }
                    Err(e) => {
                        if !bg_buffer.should_pause_prefix() {
                            bg_buffer.fail(format!("interruption flux audio : {e}"));
                        }
                        return;
                    }
                }
            }
        });

        Ok(buffer)
    }
}

/// Termine le streaming du 1er segment (si en cours), puis pré-charge les segments suivants
/// avec 1 segment d'avance en parallèle (pipelining HTTP/2) tout en remplissant `cache`.
fn spawn_dash_background(
    http: reqwest::Client,
    dash: DashStreamPlan,
    next_idx: usize,
    buffer: ProgressiveBuffer,
    cache: DashTrackCache,
    inflight_first: Option<(usize, reqwest::Response, Vec<u8>)>,
) {
    tokio::spawn(async move {
        let total = dash.segments.len();

        // Si le 1er segment n'est pas fini, lance déjà le téléchargement de `next_idx` en parallèle !
        let mut prefetched_next: Option<(usize, tokio::task::JoinHandle<Result<Vec<u8>>>)> =
            if next_idx < total && cache.get_segment(next_idx).is_none() {
                let h = http.clone();
                let u = dash.segments[next_idx].url.clone();
                Some((
                    next_idx,
                    tokio::spawn(async move { fetch_with_retry(&h, &u, 3).await }),
                ))
            } else {
                None
            };

        if let Some((first_idx, mut resp, mut acc)) = inflight_first {
            loop {
                if buffer.is_cancelled() {
                    if let Some((_, h)) = prefetched_next.take() {
                        h.abort();
                    }
                    return;
                }
                match resp.chunk().await {
                    Ok(Some(chunk)) => {
                        acc.extend_from_slice(&chunk);
                        if !buffer.push_chunk(&chunk, false) {
                            if let Some((_, h)) = prefetched_next.take() {
                                h.abort();
                            }
                            return;
                        }
                    }
                    Ok(None) => {
                        cache.set_segment(first_idx, acc);
                        buffer.push_chunk(&[], true);
                        break;
                    }
                    Err(e) => {
                        if let Some((_, h)) = prefetched_next.take() {
                            h.abort();
                        }
                        buffer.fail(format!("interruption segment #{first_idx} : {e}"));
                        return;
                    }
                }
            }
        }

        let mut idx = next_idx;
        while idx < total {
            if buffer.is_cancelled() {
                if let Some((_, h)) = prefetched_next.take() {
                    h.abort();
                }
                return;
            }

            // Démarre le prefetch de `idx + 1` en parallèle pendant qu'on attend/pousse `idx`
            let lookahead_idx = idx + 1;
            let next_prefetch = if lookahead_idx < total
                && cache.get_segment(lookahead_idx).is_none()
                && prefetched_next.as_ref().is_none_or(|(pi, _)| *pi != lookahead_idx)
            {
                let h = http.clone();
                let u = dash.segments[lookahead_idx].url.clone();
                Some((
                    lookahead_idx,
                    tokio::spawn(async move { fetch_with_retry(&h, &u, 3).await }),
                ))
            } else {
                None
            };

            let bytes_res: Result<Arc<Vec<u8>>> = if let Some(cached) = cache.get_segment(idx) {
                Ok(cached)
            } else if let Some((pi, handle)) = prefetched_next.take() {
                if pi == idx {
                    match handle.await {
                        Ok(Ok(raw)) => {
                            cache.set_segment(idx, raw.clone());
                            Ok(Arc::new(raw))
                        }
                        Ok(Err(e)) => Err(e),
                        Err(_) => fetch_with_retry(&http, &dash.segments[idx].url, 3)
                            .await
                            .map(|raw| {
                                cache.set_segment(idx, raw.clone());
                                Arc::new(raw)
                            }),
                    }
                } else {
                    handle.abort();
                    fetch_with_retry(&http, &dash.segments[idx].url, 3)
                        .await
                        .map(|raw| {
                            cache.set_segment(idx, raw.clone());
                            Arc::new(raw)
                        })
                }
            } else {
                fetch_with_retry(&http, &dash.segments[idx].url, 3)
                    .await
                    .map(|raw| {
                        cache.set_segment(idx, raw.clone());
                        Arc::new(raw)
                    })
            };

            if next_prefetch.is_some() {
                prefetched_next = next_prefetch;
            }

            match bytes_res {
                Ok(bytes) => {
                    if !buffer.push_chunk(&bytes, true) {
                        if let Some((_, h)) = prefetched_next.take() {
                            h.abort();
                        }
                        return;
                    }
                }
                Err(e) => {
                    if let Some((_, h)) = prefetched_next.take() {
                        h.abort();
                    }
                    buffer.fail(format!(
                        "échec téléchargement segment #{} : {e}",
                        dash.segments[idx].number
                    ));
                    return;
                }
            }

            idx += 1;
        }

        buffer.finish();
    });
}

/// Worker Tokio traitant les requêtes HTTP `Range: bytes=offset-` pour le seek instantané BTS.
/// Annule immédiatement toute requête Range précédente dès qu'un nouveau seek arrive.
fn spawn_range_worker(
    http: reqwest::Client,
    url: String,
    buffer: ProgressiveBuffer,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<RangeRequest>,
) {
    tokio::spawn(async move {
        let mut active_task: Option<tokio::task::JoinHandle<()>> = None;

        while let Some(mut req) = rx.recv().await {
            // Draine jusqu'à la requête la plus récente si l'utilisateur scrubbe rapidement
            while let Ok(newer) = rx.try_recv() {
                req = newer;
            }
            if let Some(prev) = active_task.take() {
                prev.abort();
            }
            if buffer.is_cancelled() {
                return;
            }

            let http = http.clone();
            let url = url.clone();
            let buf = buffer.clone();
            active_task = Some(tokio::spawn(async move {
                let range_hdr = format!("bytes={}-", req.offset);
                let resp = match http.get(&url).header("Range", &range_hdr).send().await {
                    Ok(r) => r,
                    Err(_) => {
                        buf.mark_range_unsupported();
                        return;
                    }
                };
                if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                    buf.mark_range_unsupported();
                    return;
                }
                let mut resp = resp;
                loop {
                    if buf.is_cancelled() {
                        return;
                    }
                    match resp.chunk().await {
                        Ok(Some(chunk)) => {
                            if !buf.push_range_chunk(req.generation, &chunk) {
                                return;
                            }
                        }
                        Ok(None) => {
                            buf.finish_range(req.generation);
                            return;
                        }
                        Err(_) => {
                            buf.finish_range(req.generation);
                            return;
                        }
                    }
                }
            }));
        }

        if let Some(prev) = active_task.take() {
            prev.abort();
        }
    });
}

async fn send_with_retry(
    http: &reqwest::Client,
    url: &str,
    max_attempts: u32,
) -> Result<reqwest::Response> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match http.get(url).send().await.and_then(|r| r.error_for_status()) {
            Ok(resp) => return Ok(resp),
            Err(e) if attempt < max_attempts => {
                tokio::time::sleep(Duration::from_millis(120 * u64::from(attempt))).await;
                let _ = e;
            }
            Err(e) => return Err(StreamError::Http(e)),
        }
    }
}

async fn fetch_with_retry(
    http: &reqwest::Client,
    url: &str,
    max_attempts: u32,
) -> Result<Vec<u8>> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match http.get(url).send().await.and_then(|r| r.error_for_status()) {
            Ok(resp) => match resp.bytes().await {
                Ok(b) => return Ok(b.to_vec()),
                Err(e) if attempt < max_attempts => {
                    tokio::time::sleep(Duration::from_millis(120 * u64::from(attempt))).await;
                    let _ = e;
                }
                Err(e) => return Err(StreamError::Http(e)),
            },
            Err(e) if attempt < max_attempts => {
                tokio::time::sleep(Duration::from_millis(120 * u64::from(attempt))).await;
                let _ = e;
            }
            Err(e) => return Err(StreamError::Http(e)),
        }
    }
}
