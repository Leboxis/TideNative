//! Cache asynchrone de pochettes d'albums (mémoire bornée + cache disque via `ProjectDirs`,
//! déduplication in-flight et sémaphore de concurrence CDN).

use crate::palette::extract_cover_palette;
use crate::{CacheError, Result};
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tidal_core::{CoverPalette, tidal_image_url};
use tokio::sync::{Semaphore, watch};

const MAX_MEMORY_COVERS: usize = 96;
const MAX_MEMORY_PALETTES: usize = 256;
const MAX_CONCURRENT_FETCHES: usize = 8;
const MAX_CDN_429_RETRIES: usize = 2;

type InFlightResult = std::result::Result<Arc<Vec<u8>>, String>;
type InFlightMap = HashMap<String, watch::Receiver<Option<InFlightResult>>>;

#[derive(Clone)]
pub struct CoverCache {
    dir: PathBuf,
    http: reqwest::Client,
    mem: Arc<Mutex<LruBytes>>,
    palettes: Arc<Mutex<HashMap<String, CoverPalette>>>,
    in_flight: Arc<Mutex<InFlightMap>>,
    fetch_semaphore: Arc<Semaphore>,
}

struct LruBytes {
    map: HashMap<String, Arc<Vec<u8>>>,
    order: VecDeque<String>,
}

impl LruBytes {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        self.map.get(key).cloned()
    }

    fn insert(&mut self, key: String, bytes: Arc<Vec<u8>>) {
        if self.map.contains_key(&key) {
            return;
        }
        if self.order.len() >= MAX_MEMORY_COVERS {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, bytes);
    }
}

/// Garantit le retrait de l'entrée `in_flight` et la notification des observateurs
/// même si la tâche leader est annulée.
struct InFlightGuard {
    key: String,
    map: Arc<Mutex<InFlightMap>>,
    tx: Option<watch::Sender<Option<InFlightResult>>>,
}

impl InFlightGuard {
    fn complete(mut self, result: InFlightResult) {
        self.map.lock().remove(&self.key);
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Some(result));
        }
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            self.map.lock().remove(&self.key);
            let _ = tx.send(Some(Err("téléchargement pochette annulé".into())));
        }
    }
}

impl CoverCache {
    pub fn new(dir: PathBuf, http: reqwest::Client) -> Self {
        Self::with_concurrency(dir, http, MAX_CONCURRENT_FETCHES)
    }

    pub fn with_concurrency(dir: PathBuf, http: reqwest::Client, max_concurrent: usize) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let dir = tidal_core::paths::normalize_path(&dir);
        Self {
            dir,
            http,
            mem: Arc::new(Mutex::new(LruBytes::new())),
            palettes: Arc::new(Mutex::new(HashMap::new())),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            fetch_semaphore: Arc::new(Semaphore::new(max_concurrent.max(1))),
        }
    }

    /// Chemin disque canonique pour une pochette (`uuid` ou URL OpenAPI v2, `size`).
    pub fn disk_path(&self, cover_id: &str, size: u16) -> PathBuf {
        let stem = if cover_id.starts_with("http://") || cover_id.starts_with("https://") {
            let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
            for b in cover_id.as_bytes() {
                hash ^= u64::from(*b);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            format!("url_{hash:016x}")
        } else {
            cover_id
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .take(80)
                .collect()
        };
        self.dir.join(format!("{stem}_{size}.jpg"))
    }

    /// Charge une pochette depuis le cache RAM, sinon disque, sinon CDN TIDAL
    /// (dédupliqué in-flight et borné par sémaphore). Les identifiants UUID
    /// bénéficient d'un repli de taille lorsque le CDN répond 403/404.
    pub async fn get_or_fetch(&self, cover_id: &str, size: u16) -> Result<Arc<Vec<u8>>> {
        let key = format!("{cover_id}:{size}");
        if let Some(hit) = self.mem.lock().get(&key) {
            return Ok(hit);
        }

        let path = self.disk_path(cover_id, size);
        if let Ok(bytes) = tokio::fs::read(&path).await {
            if !bytes.is_empty() {
                let arc = Arc::new(bytes);
                self.mem.lock().insert(key, Arc::clone(&arc));
                return Ok(arc);
            }
        }

        // Une URL OpenAPI v2 complète est servie telle quelle ; un UUID TIDAL
        // peut en revanche manquer à certaines tailles (photos d'artistes).
        let sizes = if cover_id.starts_with("http://") || cover_id.starts_with("https://") {
            vec![size]
        } else {
            candidate_sizes_for(size)
        };

        self.fetch_deduplicated(key, path, move |http| async move {
            fetch_with_size_fallbacks(&sizes, move |candidate| {
                let http = http.clone();
                async move {
                    let url = tidal_image_url(cover_id, candidate)
                        .ok_or_else(|| CacheError::InvalidCoverId(cover_id.to_string()))?;
                    fetch_url_bytes(&http, &url).await
                }
            })
            .await
        })
        .await
    }

    async fn fetch_deduplicated<F, Fut>(
        &self,
        key: String,
        disk_path: PathBuf,
        fetcher: F,
    ) -> Result<Arc<Vec<u8>>>
    where
        F: FnOnce(reqwest::Client) -> Fut,
        Fut: std::future::Future<Output = Result<Vec<u8>>>,
    {
        enum FlightRole {
            Leader(watch::Sender<Option<InFlightResult>>),
            Follower(watch::Receiver<Option<InFlightResult>>),
        }

        let role = {
            let mut map = self.in_flight.lock();
            if let Some(rx) = map.get(&key) {
                FlightRole::Follower(rx.clone())
            } else {
                let (tx, rx) = watch::channel(None);
                map.insert(key.clone(), rx);
                FlightRole::Leader(tx)
            }
        };

        match role {
            FlightRole::Leader(tx) => {
                let guard = InFlightGuard {
                    key: key.clone(),
                    map: Arc::clone(&self.in_flight),
                    tx: Some(tx),
                };

                let _permit = self
                    .fetch_semaphore
                    .acquire()
                    .await
                    .map_err(|_| CacheError::HttpStatus(503))?;

                // Re-vérifie RAM au cas où un autre flux vient de remplir le cache
                if let Some(hit) = self.mem.lock().get(&key) {
                    guard.complete(Ok(Arc::clone(&hit)));
                    return Ok(hit);
                }

                match fetcher(self.http.clone()).await {
                    Ok(data) => {
                        if let Some(parent) = disk_path.parent() {
                            let _ = tokio::fs::create_dir_all(parent).await;
                        }
                        let _ = tokio::fs::write(&disk_path, &data).await;
                        let arc = Arc::new(data);
                        self.mem.lock().insert(key, Arc::clone(&arc));
                        guard.complete(Ok(Arc::clone(&arc)));
                        Ok(arc)
                    }
                    Err(err) => {
                        let msg = err.to_string();
                        guard.complete(Err(msg));
                        Err(err)
                    }
                }
            }
            FlightRole::Follower(mut rx) => {
                if rx.wait_for(|v| v.is_some()).await.is_ok() {
                    if let Some(res) = rx.borrow().as_ref() {
                        return match res {
                            Ok(bytes) => Ok(Arc::clone(bytes)),
                            Err(_) => Err(CacheError::HttpStatus(502)),
                        };
                    }
                }
                Err(CacheError::HttpStatus(502))
            }
        }
    }

    pub async fn get_palette(&self, cover_id: &str) -> CoverPalette {
        if let Some(hit) = self.palettes.lock().get(cover_id).cloned() {
            return hit;
        }
        // Réutiliser immédiatement toute miniature déjà présente en RAM ou sur disque (80, 160, 320).
        let mut cached_bytes: Option<Arc<Vec<u8>>> = None;
        for sz in [80_u16, 160, 320] {
            let key = format!("{cover_id}:{sz}");
            if let Some(hit) = self.mem.lock().get(&key) {
                cached_bytes = Some(hit);
                break;
            }
            let path = self.disk_path(cover_id, sz);
            if let Ok(bytes) = tokio::fs::read(&path).await {
                if !bytes.is_empty() {
                    let arc = Arc::new(bytes);
                    self.mem.lock().insert(key, Arc::clone(&arc));
                    cached_bytes = Some(arc);
                    break;
                }
            }
        }

        let bytes = match cached_bytes {
            Some(b) => Some(b),
            None => self.get_or_fetch(cover_id, 160).await.ok(),
        };

        let Some(bytes) = bytes else {
            return CoverPalette::default();
        };

        let palette = tokio::task::spawn_blocking(move || extract_cover_palette(&bytes))
            .await
            .unwrap_or_default();
        let mut guard = self.palettes.lock();
        if guard.len() >= MAX_MEMORY_PALETTES {
            guard.clear();
        }
        guard.insert(cover_id.to_string(), palette.clone());
        palette
    }
}

/// Tailles de repli TIDAL : le CDN répond 403 sur certaines tailles
/// (photos d'artistes absentes en `80x80` / `640x640`) alors qu'elles
/// existent en `160`, `320`, `480` ou `750`.
fn candidate_sizes_for(size: u16) -> Vec<u16> {
    match size {
        80 => vec![80, 160, 320],
        160 => vec![160, 320, 480],
        320 => vec![320, 480, 160, 750],
        640 => vec![640, 750, 480, 320],
        1280 => vec![1280, 750, 640, 480, 320],
        other => vec![other, 320, 160],
    }
}

/// Essaie les tailles dans l'ordre et ne retombe sur la suivante que sur un
/// `403`/`404` CDN ; toute autre erreur est propagée immédiatement.
async fn fetch_with_size_fallbacks<F, Fut>(sizes: &[u16], fetcher: F) -> Result<Vec<u8>>
where
    F: Fn(u16) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>>>,
{
    let mut last_status: Option<CacheError> = None;
    for &size in sizes {
        match fetcher(size).await {
            Ok(bytes) => return Ok(bytes),
            Err(err) => {
                if matches!(
                    err,
                    CacheError::HttpStatus(403) | CacheError::HttpStatus(404)
                ) {
                    last_status = Some(err);
                } else {
                    return Err(err);
                }
            }
        }
    }
    Err(last_status.unwrap_or(CacheError::HttpStatus(404)))
}

async fn fetch_url_bytes(http: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    for attempt in 0..=MAX_CDN_429_RETRIES {
        let resp = http.get(url).send().await?;
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt < MAX_CDN_429_RETRIES
        {
            let delay_ms = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.trim().parse::<u64>().ok())
                .map(|s| s.saturating_mul(1000).clamp(150, 5_000))
                .unwrap_or(400 * (attempt as u64 + 1));
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            continue;
        }
        if !resp.status().is_success() {
            return Err(CacheError::HttpStatus(resp.status().as_u16()));
        }
        return Ok(resp.bytes().await?.to_vec());
    }
    Err(CacheError::HttpStatus(429))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_cache(max_concurrent: usize) -> (CoverCache, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "tidenative_cover_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cache = CoverCache::with_concurrency(dir.clone(), reqwest::Client::new(), max_concurrent);
        (cache, dir)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn deduplicates_concurrent_in_flight_fetches() {
        let (cache, dir) = temp_cache(4);
        let calls = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();

        for _ in 0..8 {
            let c = cache.clone();
            let calls_ref = Arc::clone(&calls);
            let path = c.disk_path("same-cover-uuid", 320);
            handles.push(tokio::spawn(async move {
                c.fetch_deduplicated("same-cover-uuid:320".into(), path, move |_| async move {
                    calls_ref.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    Ok(vec![1_u8, 2, 3, 4])
                })
                .await
            }));
        }

        for h in handles {
            let bytes = h.await.unwrap().unwrap();
            assert_eq!(&*bytes, &[1, 2, 3, 4]);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn semaphore_bounds_concurrent_downloads() {
        let (cache, dir) = temp_cache(2);
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();

        for i in 0..6 {
            let c = cache.clone();
            let active_ref = Arc::clone(&active);
            let peak_ref = Arc::clone(&peak);
            let key = format!("distinct-cover-{i}:320");
            let path = c.disk_path(&format!("distinct-cover-{i}"), 320);
            handles.push(tokio::spawn(async move {
                c.fetch_deduplicated(key, path, move |_| async move {
                    let cur = active_ref.fetch_add(1, Ordering::SeqCst) + 1;
                    peak_ref.fetch_max(cur, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(25)).await;
                    active_ref.fetch_sub(1, Ordering::SeqCst);
                    Ok(vec![i as u8])
                })
                .await
            }));
        }

        for h in handles {
            h.await.unwrap().unwrap();
        }
        assert!(peak.load(Ordering::SeqCst) <= 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn size_fallback_retries_only_on_403_or_404() {
        let sizes = candidate_sizes_for(80);
        assert_eq!(sizes, vec![80, 160, 320]);

        let attempted = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&attempted);
        let bytes = fetch_with_size_fallbacks(&sizes, move |size| {
            let seen = Arc::clone(&seen);
            async move {
                seen.lock().push(size);
                match size {
                    80 => Err(CacheError::HttpStatus(403)),
                    160 => Ok(vec![0xAA, 0xBB]),
                    _ => Err(CacheError::HttpStatus(500)),
                }
            }
        })
        .await
        .expect("un 403 sur 80 doit retomber sur 160 et réussir");
        assert_eq!(bytes, vec![0xAA, 0xBB]);
        assert_eq!(*attempted.lock(), vec![80, 160]);

        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let err = fetch_with_size_fallbacks(&sizes, move |_| {
            let seen = Arc::clone(&seen);
            async move {
                seen.fetch_add(1, Ordering::SeqCst);
                Err::<Vec<u8>, _>(CacheError::HttpStatus(500))
            }
        })
        .await
        .expect_err("une erreur 500 ne doit pas déclencher de repli");
        assert!(matches!(err, CacheError::HttpStatus(500)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
