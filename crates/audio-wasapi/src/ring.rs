//! Ring buffer SPSC lock-free (`rtrb`) reliant le thread de décodage FLAC au
//! thread temps réel WASAPI sans `Mutex` ni allocation mémoire.

use rtrb::{Consumer, Producer, RingBuffer};
use tidal_core::PcmFormat;

pub type PcmProducer = Producer<i32>;
pub type PcmConsumer = Consumer<i32>;

/// Crée une paire `(PcmProducer, PcmConsumer)` dimensionnée pour contenir
/// `capacity_ms` millisecondes d'audio au format `pcm`.
///
/// Exemple : 2 000 ms en 24-bit / 192 kHz stéréo = `192_000 * 2 * 2 = 768_000`
/// échantillons `i32` (~3 Mo pré-alloués une seule fois en RAM).
pub fn create_pcm_ring_buffer(pcm: PcmFormat, capacity_ms: u32) -> (PcmProducer, PcmConsumer) {
    let frames = ((u64::from(pcm.sample_rate) * u64::from(capacity_ms)) / 1000).max(1024) as usize;
    let total_samples = frames * usize::from(pcm.channels.max(1));
    RingBuffer::new(total_samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_transfers_samples_lock_free() {
        let pcm = PcmFormat::stereo(96_000, 24);
        let (mut prod, mut cons) = create_pcm_ring_buffer(pcm, 100);
        assert!(prod.slots() >= 9_600 * 2);

        if let Ok(mut chunk) = prod.write_chunk(4) {
            let (s1, s2) = chunk.as_mut_slices();
            let mut all = [0x1122_3300i32, 0x4455_6600, -0x1122_3300, -0x4455_6600];
            let l1 = s1.len();
            s1.copy_from_slice(&all[..l1]);
            s2.copy_from_slice(&mut all[l1..]);
            chunk.commit_all();
        }

        assert_eq!(cons.slots(), 4);
        assert_eq!(cons.pop().unwrap(), 0x1122_3300);
        assert_eq!(cons.pop().unwrap(), 0x4455_6600);
    }
}
