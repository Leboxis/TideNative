//! Cache asynchrone de pochettes d'albums (mémoire bornée + cache disque via `ProjectDirs`).

use crate::palette::extract_cover_palette;
use crate::{CacheError, Result};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use tidal_core::{CoverPalette, tidal_image_url};
use tokio::sync::Mutex;

const MAX_MEMORY_COVERS: usize = 96;
const MAX_MEMORY_PALETTES: usize = 256;

#[derive(Clone)]
pub struct CoverCache {
    dir: PathBuf,
    http: reqwest::Client,
    mem: Arc<Mutex<LruBytes>>,
    palettes: Arc<Mutex<HashMap<String, CoverPalette>>>,
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

impl CoverCache {
    pub fn new(dir: PathBuf, http: reqwest::Client) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let dir = tidal_core::paths::normalize_path(&dir);
        Self {
            dir,
            http,
            mem: Arc::new(Mutex::new(LruBytes::new())),
            palettes: Arc::new(Mutex::new(HashMap::new())),
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

    /// Charge une pochette depuis le cache RAM, sinon disque, sinon CDN TIDAL.
    pub async fn get_or_fetch(&self, cover_id: &str, size: u16) -> Result<Arc<Vec<u8>>> {
        let key = format!("{cover_id}:{size}");
        if let Some(hit) = self.mem.lock().await.get(&key) {
            return Ok(hit);
        }

        let path = self.disk_path(cover_id, size);
        if let Ok(bytes) = tokio::fs::read(&path).await {
            if !bytes.is_empty() {
                let arc = Arc::new(bytes);
                self.mem.lock().await.insert(key, Arc::clone(&arc));
                return Ok(arc);
            }
        }

        let url = tidal_image_url(cover_id, size)
            .ok_or_else(|| CacheError::InvalidCoverId(cover_id.to_string()))?;
        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(CacheError::HttpStatus(resp.status().as_u16()));
        }
        let data = resp.bytes().await?.to_vec();
        if let Some(parent) = path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = tokio::fs::write(&path, &data).await;

        let arc = Arc::new(data);
        self.mem.lock().await.insert(key, Arc::clone(&arc));
        Ok(arc)
    }

    pub async fn get_palette(&self, cover_id: &str) -> CoverPalette {
        if let Some(hit) = self.palettes.lock().await.get(cover_id).cloned() {
            return hit;
        }
        // Réutiliser immédiatement toute miniature déjà présente en RAM ou sur disque (80, 160, 320).
        let mut cached_bytes: Option<Arc<Vec<u8>>> = None;
        for sz in [80_u16, 160, 320] {
            let key = format!("{cover_id}:{sz}");
            if let Some(hit) = self.mem.lock().await.get(&key) {
                cached_bytes = Some(hit);
                break;
            }
            let path = self.disk_path(cover_id, sz);
            if let Ok(bytes) = tokio::fs::read(&path).await {
                if !bytes.is_empty() {
                    let arc = Arc::new(bytes);
                    self.mem.lock().await.insert(key, Arc::clone(&arc));
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

        let palette = extract_cover_palette(&bytes);
        let mut guard = self.palettes.lock().await;
        if guard.len() >= MAX_MEMORY_PALETTES {
            guard.clear();
        }
        guard.insert(cover_id.to_string(), palette.clone());
        palette
    }
}
