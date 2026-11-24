//! Tampon mémoire progressif (`ProgressiveBuffer` + `ProgressiveReader`) pour le
//! streaming audio Zéro-Disque avec support du **seek HTTP Range à la demande**
//! sur les flux Direct/BTS.
//!
//! - Flux MPEG-DASH : le thread réseau pousse `init.mp4` puis les chunks des segments
//!   fMP4/FLAC au fur et à mesure dans `data`. Le décodage démarre dès `init.mp4` + 64 Ko.
//! - Flux Direct/BTS : le préfixe `0..` arrive en streaming dans `data` ; si le décodeur
//!   effectue un `seek()` loin en avant, `ProgressiveReader` déclenche immédiatement une
//!   requête HTTP `Range: bytes=offset-` via `range_tx` sans attendre le téléchargement
//!   séquentiel des dizaines de Mo intermédiaires.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Condvar, Mutex};
use tokio::sync::mpsc;

/// Requête de téléchargement HTTP Range déclenchée lors d'un seek loin en avant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeRequest {
    pub offset: u64,
    pub generation: u64,
}

const RANGE_LOOKAHEAD_BYTES: usize = 256 * 1024;

#[derive(Default)]
struct InnerState {
    data: Vec<u8>,
    total_expected: Option<u64>,
    segments_loaded: usize,
    total_segments: usize,
    eof: bool,
    cancelled: bool,
    error: Option<String>,

    // --- Support HTTP Range à la demande pour seek instantané sur flux Direct/BTS ---
    range_tx: Option<mpsc::UnboundedSender<RangeRequest>>,
    range_supported: bool,
    range_gen: u64,
    range_start: usize,
    range_data: Vec<u8>,
    range_eof: bool,
    range_active: bool,
    pause_prefix: bool,
}

impl InnerState {
    fn trigger_range(&mut self, offset: usize) -> bool {
        let Some(tx) = &self.range_tx else {
            return false;
        };
        if !self.range_supported {
            return false;
        }
        self.range_gen = self.range_gen.wrapping_add(1).max(1);
        self.range_start = offset;
        self.range_data.clear();
        self.range_eof = false;
        self.range_active = true;
        let _ = tx.send(RangeRequest {
            offset: offset as u64,
            generation: self.range_gen,
        });
        true
    }
}

/// Producteur/état partagé du tampon mémoire progressif.
#[derive(Clone, Default)]
pub struct ProgressiveBuffer {
    inner: Arc<(Mutex<InnerState>, Condvar)>,
}

impl ProgressiveBuffer {
    /// Crée un tampon pré-alloué avec une capacité initiale en octets.
    pub fn with_capacity(capacity_bytes: usize, total_segments: usize) -> Self {
        let state = InnerState {
            data: Vec::with_capacity(capacity_bytes),
            total_expected: None,
            segments_loaded: 0,
            total_segments,
            eof: false,
            cancelled: false,
            error: None,
            range_tx: None,
            range_supported: false,
            range_gen: 0,
            range_start: 0,
            range_data: Vec::new(),
            range_eof: false,
            range_active: false,
            pause_prefix: false,
        };
        Self {
            inner: Arc::new((Mutex::new(state), Condvar::new())),
        }
    }

    /// Crée un tampon déjà complet à partir d'un vecteur d'octets en mémoire
    /// (utile pour les tests hors-ligne ou les pistes déjà en cache).
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        let len = bytes.len() as u64;
        let state = InnerState {
            data: bytes,
            total_expected: Some(len),
            segments_loaded: 1,
            total_segments: 1,
            eof: true,
            cancelled: false,
            error: None,
            range_tx: None,
            range_supported: false,
            range_gen: 0,
            range_start: 0,
            range_data: Vec::new(),
            range_eof: false,
            range_active: false,
            pause_prefix: false,
        };
        Self {
            inner: Arc::new((Mutex::new(state), Condvar::new())),
        }
    }

    /// Active le mécanisme de requêtes HTTP `Range` à la demande (flux Direct/BTS).
    pub fn enable_range_seeking(&self) -> mpsc::UnboundedReceiver<RangeRequest> {
        let (tx, rx) = mpsc::unbounded_channel();
        let (lock, _) = &*self.inner;
        let mut st = lock.lock().expect("ProgressiveBuffer mutex poisoned");
        st.range_tx = Some(tx);
        st.range_supported = true;
        rx
    }

    /// Ajoute un bloc d'octets au flux principal (`data`) et réveille le décodeur.
    pub fn push_chunk(&self, chunk: &[u8], mark_segment: bool) -> bool {
        let (lock, cvar) = &*self.inner;
        let mut st = lock.lock().expect("ProgressiveBuffer mutex poisoned");
        if st.cancelled {
            return false;
        }
        st.data.extend_from_slice(chunk);
        if mark_segment {
            st.segments_loaded += 1;
        }
        cvar.notify_all();
        true
    }

    /// Indique au téléchargeur de préfixe s'il doit se mettre en pause pour céder
    /// 100 % de la bande passante à un flux Range actif.
    pub fn should_pause_prefix(&self) -> bool {
        let (lock, _) = &*self.inner;
        let st = lock.lock().expect("ProgressiveBuffer mutex poisoned");
        st.pause_prefix && st.range_supported
    }

    /// Pousse des octets provenant d'une requête HTTP `Range` active (`generation`).
    /// Renvoie `false` si le tampon est annulé ou si une requête plus récente a remplacé celle-ci.
    pub fn push_range_chunk(&self, generation: u64, chunk: &[u8]) -> bool {
        let (lock, cvar) = &*self.inner;
        let mut st = lock.lock().expect("ProgressiveBuffer mutex poisoned");
        if st.cancelled || st.range_gen != generation {
            return false;
        }
        st.range_data.extend_from_slice(chunk);
        // Une fois que le flux Range confirme un vrai streaming (206 + >128 Ko),
        // on libère la bande passante du flux préfixe initial.
        if st.range_data.len() >= 128 * 1024 {
            st.pause_prefix = true;
        }
        cvar.notify_all();
        true
    }

    /// Marque la fin normale du flux HTTP `Range` courant.
    pub fn finish_range(&self, generation: u64) {
        let (lock, cvar) = &*self.inner;
        let mut st = lock.lock().expect("ProgressiveBuffer mutex poisoned");
        if st.range_gen == generation {
            st.range_eof = true;
            cvar.notify_all();
        }
    }

    /// Désactive le mode HTTP `Range` si le serveur ne retourne pas `206 Partial Content`.
    pub fn mark_range_unsupported(&self) {
        let (lock, cvar) = &*self.inner;
        let mut st = lock.lock().expect("ProgressiveBuffer mutex poisoned");
        st.range_supported = false;
        st.range_active = false;
        st.pause_prefix = false;
        cvar.notify_all();
    }

    /// Renseigne la taille totale attendue si elle est connue (`Content-Length` d'un flux BTS).
    pub fn set_total_expected(&self, len: u64) {
        let (lock, _) = &*self.inner;
        let mut st = lock.lock().expect("ProgressiveBuffer mutex poisoned");
        st.total_expected = Some(len);
    }

    /// Marque la fin normale du téléchargement principal.
    pub fn finish(&self) {
        let (lock, cvar) = &*self.inner;
        let mut st = lock.lock().expect("ProgressiveBuffer mutex poisoned");
        st.eof = true;
        if st.total_expected.is_none() {
            st.total_expected = Some(st.data.len() as u64);
        }
        cvar.notify_all();
    }

    /// Signale une erreur réseau irrécupérable et débloque le lecteur.
    pub fn fail(&self, message: impl Into<String>) {
        let (lock, cvar) = &*self.inner;
        let mut st = lock.lock().expect("ProgressiveBuffer mutex poisoned");
        st.error = Some(message.into());
        st.eof = true;
        cvar.notify_all();
    }

    /// Interrompt immédiatement le flux (changement de piste / arrêt).
    pub fn cancel(&self) {
        let (lock, cvar) = &*self.inner;
        let mut st = lock.lock().expect("ProgressiveBuffer mutex poisoned");
        st.cancelled = true;
        st.eof = true;
        st.range_tx = None;
        cvar.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        let (lock, _) = &*self.inner;
        lock.lock().expect("ProgressiveBuffer mutex poisoned").cancelled
    }

    /// Nombre d'octets actuellement disponibles en RAM dans le préfixe.
    pub fn downloaded_bytes(&self) -> usize {
        let (lock, _) = &*self.inner;
        lock.lock().expect("ProgressiveBuffer mutex poisoned").data.len()
    }

    /// Nombre de segments média déjà téléchargés.
    pub fn segments_progress(&self) -> (usize, usize) {
        let (lock, _) = &*self.inner;
        let st = lock.lock().expect("ProgressiveBuffer mutex poisoned");
        (st.segments_loaded, st.total_segments)
    }

    /// Indique si le téléchargement est terminé (`eof`).
    pub fn is_finished(&self) -> bool {
        let (lock, _) = &*self.inner;
        lock.lock().expect("ProgressiveBuffer mutex poisoned").eof
    }

    /// Ouvre un curseur de lecture `Read + Seek` pour le thread décodeur.
    pub fn reader(&self) -> ProgressiveReader {
        ProgressiveReader {
            inner: Arc::clone(&self.inner),
            pos: 0,
        }
    }
}

/// Lecteur synchrone (`Read + Seek + Send + Sync`) branché sur un [`ProgressiveBuffer`].
pub struct ProgressiveReader {
    inner: Arc<(Mutex<InnerState>, Condvar)>,
    pos: usize,
}

impl ProgressiveReader {
    /// Taille totale du flux si le téléchargement est terminé ou si `Content-Length` est connu.
    pub fn byte_len(&self) -> Option<u64> {
        let (lock, _) = &*self.inner;
        let st = lock.lock().expect("ProgressiveBuffer mutex poisoned");
        if st.eof {
            Some(st.total_expected.unwrap_or(st.data.len() as u64))
        } else {
            st.total_expected
        }
    }
}

impl Read for ProgressiveReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let (lock, cvar) = &*self.inner;
        let mut st = lock.lock().expect("ProgressiveBuffer mutex poisoned");
        loop {
            if st.cancelled {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "flux annulé"));
            }
            if let Some(err) = &st.error {
                return Err(io::Error::other(err.clone()));
            }

            // 1. Données disponibles dans le préfixe contigu (0..data.len())
            if self.pos < st.data.len() {
                let avail = &st.data[self.pos..];
                let to_copy = avail.len().min(buf.len());
                buf[..to_copy].copy_from_slice(&avail[..to_copy]);
                self.pos += to_copy;
                return Ok(to_copy);
            }

            // 2. Données disponibles dans la fenêtre Range active
            if st.range_active && self.pos >= st.range_start {
                let rel = self.pos - st.range_start;
                if rel < st.range_data.len() {
                    let avail = &st.range_data[rel..];
                    let to_copy = avail.len().min(buf.len());
                    buf[..to_copy].copy_from_slice(&avail[..to_copy]);
                    self.pos += to_copy;
                    return Ok(to_copy);
                }
                if st.range_eof {
                    return Ok(0);
                }
            }

            // 3. Fin de fichier atteinte
            if let Some(total) = st.total_expected {
                if (self.pos as u64) >= total {
                    return Ok(0);
                }
            }
            if st.eof && (!st.range_active || !st.range_supported) {
                return Ok(0);
            }

            // 4. Si le curseur pointe hors du préfixe et hors de la fenêtre Range en cours,
            // déclenche une requête HTTP Range immédiatement.
            if st.range_supported && st.range_tx.is_some() {
                let near_prefix =
                    !st.pause_prefix && !st.eof && self.pos <= st.data.len() + RANGE_LOOKAHEAD_BYTES;
                let near_range = st.range_active
                    && !st.range_eof
                    && self.pos >= st.range_start
                    && self.pos <= st.range_start + st.range_data.len() + RANGE_LOOKAHEAD_BYTES;
                if !near_prefix && !near_range {
                    st.trigger_range(self.pos);
                }
            }

            st = cvar
                .wait(st)
                .expect("ProgressiveBuffer condvar wait poisoned");
        }
    }
}

impl Seek for ProgressiveReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let (lock, cvar) = &*self.inner;
        let mut st = lock.lock().expect("ProgressiveBuffer mutex poisoned");

        let target: i128 = match pos {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::Current(delta) => (self.pos as i128) + i128::from(delta),
            SeekFrom::End(delta) => {
                while !st.eof && st.total_expected.is_none() && !st.cancelled {
                    st = cvar
                        .wait(st)
                        .expect("ProgressiveBuffer condvar wait poisoned");
                }
                let end_len = st.total_expected.unwrap_or(st.data.len() as u64) as i128;
                end_len + i128::from(delta)
            }
        };

        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek avant le début du flux",
            ));
        }

        let mut target_usize = target as usize;
        if let Some(total) = st.total_expected {
            target_usize = target_usize.min(total as usize);
        }

        // Mode HTTP Range actif : seek non-bloquant immédiat + pré-déclenchement Range si loin en avant
        if st.range_supported && st.range_tx.is_some() {
            let in_prefix = target_usize <= st.data.len();
            let near_prefix =
                !st.pause_prefix && !st.eof && target_usize <= st.data.len() + RANGE_LOOKAHEAD_BYTES;
            let in_or_near_range = st.range_active
                && target_usize >= st.range_start
                && target_usize <= st.range_start + st.range_data.len() + RANGE_LOOKAHEAD_BYTES;

            if !in_prefix && !near_prefix && !in_or_near_range {
                st.trigger_range(target_usize);
            }
            self.pos = target_usize;
            return Ok(self.pos as u64);
        }

        // Mode séquentiel sans Range : attend l'arrivée des octets
        while target_usize > st.data.len() && !st.eof && !st.cancelled {
            if let Some(err) = &st.error {
                return Err(io::Error::other(err.clone()));
            }
            st = cvar
                .wait(st)
                .expect("ProgressiveBuffer condvar wait poisoned");
        }

        self.pos = target_usize.min(st.data.len());
        Ok(self.pos as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn progressive_reader_waits_for_background_producer() {
        let buf = ProgressiveBuffer::with_capacity(64, 2);
        let producer = buf.clone();

        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(15));
            producer.push_chunk(b"fLaC-init-", false);
            thread::sleep(Duration::from_millis(15));
            producer.push_chunk(b"segment1", true);
            producer.finish();
        });

        let mut reader = buf.reader();
        let mut out = Vec::new();
        reader.read_to_end(&mut out).unwrap();
        handle.join().unwrap();

        assert_eq!(out, b"fLaC-init-segment1");
        assert_eq!(buf.segments_progress(), (1, 2));
    }

    #[test]
    fn range_seek_triggers_on_demand_fetch_without_waiting_for_full_prefix() {
        let buf = ProgressiveBuffer::with_capacity(64, 1);
        buf.set_total_expected(1_000_000);
        buf.push_chunk(b"HEADER_BYTES", false);
        let mut rx = buf.enable_range_seeking();

        let producer = buf.clone();
        let handle = thread::spawn(move || {
            let req = rx.blocking_recv().expect("range request expected");
            assert_eq!(req.offset, 500_000);
            producer.push_range_chunk(req.generation, b"FAR_AUDIO_FRAME");
            producer.finish_range(req.generation);
        });

        let mut reader = buf.reader();
        let pos = reader.seek(SeekFrom::Start(500_000)).unwrap();
        assert_eq!(pos, 500_000);

        let mut out = [0u8; 15];
        reader.read_exact(&mut out).unwrap();
        assert_eq!(&out, b"FAR_AUDIO_FRAME");

        // Retour immédiat au préfixe (en-tête) sans requête réseau
        reader.seek(SeekFrom::Start(0)).unwrap();
        let mut hdr = [0u8; 12];
        reader.read_exact(&mut hdr).unwrap();
        assert_eq!(&hdr, b"HEADER_BYTES");

        handle.join().unwrap();
    }
}
