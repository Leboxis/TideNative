//! Formats matériels WASAPI (`WAVEFORMATEXTENSIBLE`) et conversion sans allocation
//! depuis le format interne `i32` aligné MSB vers le tampon DMA du DAC.

use tidal_core::PcmFormat;

/// Format binaire des échantillons attendus par le tampon matériel `IAudioRenderClient`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareSampleFormat {
    /// Entier signé 16-bit little-endian (2 octets / échantillon).
    S16,
    /// Entier signé 24-bit compacté little-endian (3 octets / échantillon).
    S24Packed,
    /// Entier signé 24-bit dans un mot de 32-bit aligné MSB (`wBitsPerSample=32`, `wValidBitsPerSample=24`).
    S24In32,
    /// Entier signé 32-bit little-endian (4 octets / échantillon).
    S32,
    /// Flottant IEEE 754 32-bit `[-1.0, 1.0]` (utilisé uniquement en repli WASAPI Partagé).
    F32,
}

impl HardwareSampleFormat {
    /// Taille du conteneur matériel en octets par échantillon mono.
    pub const fn bytes_per_sample(self) -> usize {
        match self {
            Self::S16 => 2,
            Self::S24Packed => 3,
            Self::S24In32 | Self::S32 | Self::F32 => 4,
        }
    }

    /// `wBitsPerSample` dans `WAVEFORMATEX`.
    pub const fn container_bits(self) -> u16 {
        (self.bytes_per_sample() * 8) as u16
    }

    /// `wValidBitsPerSample` dans `WAVEFORMATEXTENSIBLE`.
    pub const fn valid_bits(self) -> u16 {
        match self {
            Self::S16 => 16,
            Self::S24Packed | Self::S24In32 => 24,
            Self::S32 | Self::F32 => 32,
        }
    }

    /// Indique si ce conteneur préserve 100 % des bits d'une source de profondeur `source_bits`.
    pub const fn is_bit_perfect_for(self, source_bits: u16) -> bool {
        match self {
            Self::F32 => false,
            _ => self.valid_bits() >= source_bits,
        }
    }
}

/// Résultat de la négociation de format avec le driver WASAPI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NegotiatedFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: HardwareSampleFormat,
    pub is_exclusive: bool,
    pub buffer_frames: u32,
}

impl NegotiatedFormat {
    /// Taille en octets d'une trame multicanal dans le tampon DMA WASAPI.
    pub const fn bytes_per_frame(self) -> usize {
        (self.channels as usize) * self.sample_format.bytes_per_sample()
    }
}

/// Ordre de priorité des formats candidats pour `IAudioClient::IsFormatSupported`
/// en mode WASAPI Exclusif, selon la profondeur de bits de la piste source.
pub fn candidate_exclusive_formats(source: PcmFormat) -> [HardwareSampleFormat; 4] {
    match source.bits_per_sample {
        0..=16 => [
            HardwareSampleFormat::S16,
            HardwareSampleFormat::S24In32,
            HardwareSampleFormat::S24Packed,
            HardwareSampleFormat::S32,
        ],
        17..=24 => [
            // La quasi-totalité des DAC USB Audio Class 2 (XMOS, Amanero, Topping, SMSL, RME)
            // utilisent un conteneur 32-bit avec 24 bits valides alignés MSB.
            HardwareSampleFormat::S24In32,
            HardwareSampleFormat::S24Packed,
            HardwareSampleFormat::S32,
            HardwareSampleFormat::S16,
        ],
        _ => [
            HardwareSampleFormat::S32,
            HardwareSampleFormat::S24In32,
            HardwareSampleFormat::S24Packed,
            HardwareSampleFormat::S16,
        ],
    }
}

/// Copie et convertit à la volée des échantillons `i32` alignés MSB vers le tampon
/// matériel WASAPI (`dst_bytes`), **sans aucune allocation mémoire**.
///
/// Précondition : `dst_bytes.len() >= src_msb_i32.len() * format.bytes_per_sample()`.
#[inline]
pub fn pack_msb_i32_to_hardware(
    src_msb_i32: &[i32],
    dst_bytes: &mut [u8],
    format: HardwareSampleFormat,
) {
    match format {
        HardwareSampleFormat::S24In32 => {
            // En S24In32 MSB-aligned, le mot de 32 bits avec les 8 bits de poids faible
            // masqués (`& !0xFF`) est directement au format attendu par le DMA USB Audio Class 2.
            for (sample, out_chunk) in src_msb_i32.iter().zip(dst_bytes.chunks_exact_mut(4)) {
                let masked = *sample & !0xFF;
                out_chunk.copy_from_slice(&masked.to_le_bytes());
            }
        }
        HardwareSampleFormat::S32 => {
            for (sample, out_chunk) in src_msb_i32.iter().zip(dst_bytes.chunks_exact_mut(4)) {
                out_chunk.copy_from_slice(&sample.to_le_bytes());
            }
        }
        HardwareSampleFormat::S24Packed => {
            // Les 24 bits utiles sont dans les octets [1, 2, 3] de la représentation little-endian
            // de l'entier 32 bits aligné MSB.
            for (sample, out_chunk) in src_msb_i32.iter().zip(dst_bytes.chunks_exact_mut(3)) {
                let le = sample.to_le_bytes();
                out_chunk[0] = le[1];
                out_chunk[1] = le[2];
                out_chunk[2] = le[3];
            }
        }
        HardwareSampleFormat::S16 => {
            for (sample, out_chunk) in src_msb_i32.iter().zip(dst_bytes.chunks_exact_mut(2)) {
                let s16 = (*sample >> 16) as i16;
                out_chunk.copy_from_slice(&s16.to_le_bytes());
            }
        }
        HardwareSampleFormat::F32 => {
            const INV_SCALE: f64 = 1.0 / 2_147_483_648.0;
            for (sample, out_chunk) in src_msb_i32.iter().zip(dst_bytes.chunks_exact_mut(4)) {
                let f = (f64::from(*sample) * INV_SCALE) as f32;
                out_chunk.copy_from_slice(&f.to_le_bytes());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_s24_in_32_and_s24_packed_bit_exact() {
        let s24_val: i32 = 0x005A_BCDE;
        let msb_i32: i32 = s24_val << 8; // 0x5ABC_DE00

        let mut buf_32 = [0u8; 4];
        pack_msb_i32_to_hardware(&[msb_i32], &mut buf_32, HardwareSampleFormat::S24In32);
        assert_eq!(i32::from_le_bytes(buf_32), 0x5ABC_DE00);

        let mut buf_24 = [0u8; 3];
        pack_msb_i32_to_hardware(&[msb_i32], &mut buf_24, HardwareSampleFormat::S24Packed);
        assert_eq!(buf_24, [0xDE, 0xBC, 0x5A]);

        let s16_val: i16 = -12_345;
        let msb_16: i32 = i32::from(s16_val) << 16;
        let mut buf_16 = [0u8; 2];
        pack_msb_i32_to_hardware(&[msb_16], &mut buf_16, HardwareSampleFormat::S16);
        assert_eq!(i16::from_le_bytes(buf_16), s16_val);
    }

    #[test]
    fn prioritizes_s24_in_32_for_24bit_sources() {
        let candidates = candidate_exclusive_formats(PcmFormat::stereo(192_000, 24));
        assert_eq!(candidates[0], HardwareSampleFormat::S24In32);
        assert_eq!(candidates[1], HardwareSampleFormat::S24Packed);
    }
}
