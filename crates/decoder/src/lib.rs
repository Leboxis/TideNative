//! Décodeur audio Bit-Perfect en mémoire basé sur `symphonia` (`flac`, `isomp4`,
//! `aac`, `pcm`, `wav`, `opt-simd`).
//!
//! Tous les échantillons décodés sont normalisés sans aucune perte en entiers
//! signés 32 bits **alignés sur le bit de poids fort (MSB-aligned `i32`)** :
//! - un échantillon **16-bit** `s16` est stocké sous la forme `(s16 as i32) << 16` ;
//! - un échantillon **24-bit** `s24` est stocké sous la forme `s24.inner() << 8` ;
//! - un échantillon **32-bit** `s32` est stocké tel quel.
//!
//! Cette convention est 100 % bit-exacte et correspond directement au format
//! matériel `WAVEFORMATEXTENSIBLE` (`wBitsPerSample = 32, wValidBitsPerSample = 24`)
//! utilisé par les DAC USB Audio Class 2 en mode WASAPI Exclusif.

use std::io::{Read, Seek, SeekFrom};
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{
    CODEC_TYPE_AAC, CODEC_TYPE_FLAC, CODEC_TYPE_MP3, CodecType, Decoder, DecoderOptions,
};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;
use tidal_core::PcmFormat;
use tidal_stream::{ProgressiveBuffer, ProgressiveReader};

#[derive(Debug, thiserror::Error)]
pub enum DecoderError {
    #[error("erreur de décodage audio Symphonia : {0}")]
    Symphonia(#[from] SymphoniaError),

    #[error("aucune piste audio exploitable trouvée dans le flux")]
    NoAudioTrack,

    #[error("format PCM invalide ou non supporté (sample_rate={sample_rate}, channels={channels})")]
    UnsupportedFormat { sample_rate: u32, channels: u16 },
}

pub type Result<T> = std::result::Result<T, DecoderError>;

/// Adaptateur [`MediaSource`] branché sur un [`ProgressiveReader`].
///
/// Pour un flux MPEG-DASH fMP4 en cours de téléchargement (`seekable = false`),
/// désactiver `is_seekable()` permet à `IsoMp4Reader` de démarrer la lecture dès
/// la réception de `init.mp4` (`ftyp` + `moov`) sans scanner jusqu'à la fin du
/// dernier segment `moof`/`mdat`.
pub struct ProgressiveMediaSource {
    reader: ProgressiveReader,
    seekable: bool,
}

impl ProgressiveMediaSource {
    pub fn new(reader: ProgressiveReader, seekable: bool) -> Self {
        Self { reader, seekable }
    }
}

impl Read for ProgressiveMediaSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buf)
    }
}

impl Seek for ProgressiveMediaSource {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.reader.seek(pos)
    }
}

impl MediaSource for ProgressiveMediaSource {
    fn is_seekable(&self) -> bool {
        self.seekable
    }

    fn byte_len(&self) -> Option<u64> {
        if self.seekable {
            self.reader.byte_len()
        } else {
            None
        }
    }
}

/// Décodeur de flux audio produisant des trames PCM entrelacées (`i32` MSB-aligned).
pub struct StreamDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    pcm_format: PcmFormat,
    codec_label: String,
    total_frames: Option<u64>,
}

impl StreamDecoder {
    /// Ouvre un flux audio depuis un [`ProgressiveBuffer`].
    ///
    /// - `mime_type` : ex. `Some("audio/mp4")` (DASH Hi-Res) ou `Some("audio/flac")` (BTS).
    /// - `is_dash_fmp4` : si `true`, configure la source en mode flux fMP4 progressif
    ///   pour un démarrage immédiat dès `init.mp4` + segment 1.
    pub fn open(
        buffer: &ProgressiveBuffer,
        mime_type: Option<&str>,
        is_dash_fmp4: bool,
    ) -> Result<Self> {
        let seekable = !is_dash_fmp4;
        let source = ProgressiveMediaSource::new(buffer.reader(), seekable);
        let mss = MediaSourceStream::new(Box::new(source), Default::default());

        let mut hint = Hint::new();
        if let Some(mime) = mime_type {
            hint.mime_type(mime);
            if mime.contains("flac") {
                hint.with_extension("flac");
            } else if mime.contains("mp4") {
                hint.with_extension("mp4");
            }
        }

        let fmt_opts = FormatOptions {
            enable_gapless: true,
            ..Default::default()
        };
        let meta_opts = MetadataOptions::default();

        let probed = symphonia::default::get_probe().format(&hint, mss, &fmt_opts, &meta_opts)?;
        let format = probed.format;

        let track = format
            .default_track()
            .or_else(|| format.tracks().first())
            .ok_or(DecoderError::NoAudioTrack)?;

        let track_id = track.id;
        let params = &track.codec_params;

        let sample_rate = params.sample_rate.unwrap_or(44_100);
        let channels = params
            .channels
            .map(|c| c.count() as u16)
            .or_else(|| params.channel_layout.map(|l| l.into_channels().count() as u16))
            .unwrap_or(2);

        if sample_rate == 0 || channels == 0 {
            return Err(DecoderError::UnsupportedFormat {
                sample_rate,
                channels,
            });
        }

        let bits_per_sample = match params.bits_per_sample.or(params.bits_per_coded_sample) {
            Some(b) if b <= 16 => 16,
            Some(b) if b <= 24 => 24,
            Some(_) => 32,
            None => 24, // AAC / flottant converti en 24-bit MSB
        };

        let codec_label = codec_display_name(params.codec).to_string();
        let total_frames = params.n_frames;

        let decoder = symphonia::default::get_codecs()
            .make(params, &DecoderOptions::default())?;

        Ok(Self {
            format,
            decoder,
            track_id,
            pcm_format: PcmFormat {
                sample_rate,
                channels,
                bits_per_sample,
            },
            codec_label,
            total_frames,
        })
    }

    /// Format PCM détecté dans les en-têtes du flux (`STREAMINFO` FLAC / `moov` fMP4).
    pub fn pcm_format(&self) -> PcmFormat {
        self.pcm_format
    }

    /// Nom lisible du codec (`"FLAC"`, `"AAC"`, `"PCM"`, etc.).
    pub fn codec_label(&self) -> &str {
        &self.codec_label
    }

    /// Durée totale connue en millisecondes, si disponible dans l'en-tête.
    pub fn duration_ms(&self) -> Option<u64> {
        let frames = self.total_frames?;
        Some(self.pcm_format.frames_to_ms(frames))
    }

    /// Décode le prochain paquet audio et ajoute les échantillons entrelacés
    /// (`i32` alignés MSB) dans `out_samples`.
    ///
    /// Renvoie :
    /// - `Ok(Some(frames))` : nombre de trames multicanal ajoutées ;
    /// - `Ok(None)` : fin de piste atteinte (`EOF`).
    pub fn decode_next(&mut self, out_samples: &mut Vec<i32>) -> Result<Option<usize>> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(pkt) => pkt,
                Err(SymphoniaError::IoError(ref io_err))
                    if io_err.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(e) => return Err(DecoderError::Symphonia(e)),
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(audio_buf) => {
                    // Met à jour dynamiquement sample_rate / channels si le premier paquet précise le spec
                    let spec = *audio_buf.spec();
                    if spec.rate != self.pcm_format.sample_rate && spec.rate > 0 {
                        self.pcm_format.sample_rate = spec.rate;
                    }
                    let ch = spec.channels.count() as u16;
                    if ch != self.pcm_format.channels && ch > 0 {
                        self.pcm_format.channels = ch;
                    }

                    let frames = append_msb_interleaved(audio_buf, out_samples);
                    return Ok(Some(frames));
                }
                Err(SymphoniaError::DecodeError(_)) => {
                    // Trame corrompue isolée : on continue sans interrompre la lecture
                    continue;
                }
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(e) => return Err(DecoderError::Symphonia(e)),
            }
        }
    }

    /// Repositionne le décodeur à `position_ms` (quand le conteneur est seekable).
    pub fn seek_ms(&mut self, position_ms: u64) -> Result<u64> {
        let secs = position_ms / 1000;
        let frac = (position_ms % 1000) as f64 / 1000.0;
        let seeked = self.format.seek(
            SeekMode::Coarse,
            SeekTo::Time {
                time: Time::new(secs, frac),
                track_id: Some(self.track_id),
            },
        )?;
        self.decoder.reset();
        Ok(self.pcm_format.frames_to_ms(seeked.actual_ts))
    }
}

fn codec_display_name(codec: CodecType) -> &'static str {
    if codec == CODEC_TYPE_FLAC {
        "FLAC"
    } else if codec == CODEC_TYPE_AAC {
        "AAC"
    } else if codec == CODEC_TYPE_MP3 {
        "MP3"
    } else {
        "PCM"
    }
}

/// Convertit sans perte un [`AudioBufferRef`] Symphonia en échantillons `i32`
/// entrelacés alignés MSB (32-bit full-scale).
fn append_msb_interleaved(buf: AudioBufferRef<'_>, out: &mut Vec<i32>) -> usize {
    let frames = buf.frames();
    let channels = buf.spec().channels.count();
    out.reserve(frames * channels);

    match buf {
        AudioBufferRef::S16(b) => {
            for frame_idx in 0..frames {
                for ch in 0..channels {
                    let s16 = b.chan(ch)[frame_idx];
                    out.push(i32::from(s16) << 16);
                }
            }
        }
        AudioBufferRef::S24(b) => {
            for frame_idx in 0..frames {
                for ch in 0..channels {
                    let s24 = b.chan(ch)[frame_idx].inner();
                    out.push(s24 << 8);
                }
            }
        }
        AudioBufferRef::S32(b) => {
            for frame_idx in 0..frames {
                for ch in 0..channels {
                    out.push(b.chan(ch)[frame_idx]);
                }
            }
        }
        AudioBufferRef::F32(b) => {
            const SCALE: f64 = 2_147_483_647.0;
            for frame_idx in 0..frames {
                for ch in 0..channels {
                    let v = f64::from(b.chan(ch)[frame_idx]).clamp(-1.0, 1.0);
                    out.push((v * SCALE) as i32);
                }
            }
        }
        AudioBufferRef::F64(b) => {
            const SCALE: f64 = 2_147_483_647.0;
            for frame_idx in 0..frames {
                for ch in 0..channels {
                    let v = b.chan(ch)[frame_idx].clamp(-1.0, 1.0);
                    out.push((v * SCALE) as i32);
                }
            }
        }
        AudioBufferRef::U8(b) => {
            for frame_idx in 0..frames {
                for ch in 0..channels {
                    let centered = i32::from(b.chan(ch)[frame_idx]) - 128;
                    out.push(centered << 24);
                }
            }
        }
        AudioBufferRef::U16(b) => {
            for frame_idx in 0..frames {
                for ch in 0..channels {
                    let centered = i32::from(b.chan(ch)[frame_idx]) - 32_768;
                    out.push(centered << 16);
                }
            }
        }
        AudioBufferRef::U24(b) => {
            for frame_idx in 0..frames {
                for ch in 0..channels {
                    let centered = (b.chan(ch)[frame_idx].inner() as i32) - 8_388_608;
                    out.push(centered << 8);
                }
            }
        }
        AudioBufferRef::U32(b) => {
            for frame_idx in 0..frames {
                for ch in 0..channels {
                    let u = b.chan(ch)[frame_idx];
                    out.push((u as i64 - 2_147_483_648) as i32);
                }
            }
        }
        AudioBufferRef::S8(b) => {
            for frame_idx in 0..frames {
                for ch in 0..channels {
                    out.push(i32::from(b.chan(ch)[frame_idx]) << 24);
                }
            }
        }
    }

    frames
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Génère un fichier WAV PCM 24-bit / 96 kHz stéréo minimal en mémoire
    /// pour vérifier que le décodage Symphonia -> MSB `i32` est 100 % bit-exact.
    fn make_24bit_96khz_wav(samples_24bit_stereo: &[(i32, i32)]) -> Vec<u8> {
        let sample_rate: u32 = 96_000;
        let channels: u16 = 2;
        let bits_per_sample: u16 = 24;
        let bytes_per_sample: u16 = 3;
        let block_align: u16 = channels * bytes_per_sample;
        let byte_rate: u32 = sample_rate * u32::from(block_align);
        let data_size: u32 = (samples_24bit_stereo.len() * usize::from(block_align)) as u32;
        let riff_size: u32 = 36 + data_size;

        let mut wav = Vec::with_capacity((44 + data_size) as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&riff_size.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM integer
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&block_align.to_le_bytes());
        wav.extend_from_slice(&bits_per_sample.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());

        for &(l, r) in samples_24bit_stereo {
            let lb = l.to_le_bytes();
            let rb = r.to_le_bytes();
            wav.extend_from_slice(&[lb[0], lb[1], lb[2]]);
            wav.extend_from_slice(&[rb[0], rb[1], rb[2]]);
        }
        wav
    }

    #[test]
    fn decodes_24bit_96khz_stream_bit_perfectly_to_msb_i32() {
        let original_frames = vec![
            (0x0012_3456, -0x0065_4321),
            (0x007F_FFFF, -0x0080_0000),
            (0, 42),
        ];
        let wav_bytes = make_24bit_96khz_wav(&original_frames);
        let progressive = ProgressiveBuffer::from_bytes(wav_bytes);

        let mut dec = StreamDecoder::open(&progressive, Some("audio/wav"), false)
            .expect("decoder should open 24/96 WAV");
        assert_eq!(dec.pcm_format(), PcmFormat::stereo(96_000, 24));

        let mut pcm_out = Vec::new();
        while dec.decode_next(&mut pcm_out).unwrap().is_some() {}

        assert_eq!(pcm_out.len(), original_frames.len() * 2);
        for (idx, &(expected_l, expected_r)) in original_frames.iter().enumerate() {
            let decoded_l = pcm_out[idx * 2] >> 8;
            let decoded_r = pcm_out[idx * 2 + 1] >> 8;
            assert_eq!(decoded_l, expected_l, "canal gauche trame {idx} non bit-perfect");
            assert_eq!(decoded_r, expected_r, "canal droit trame {idx} non bit-perfect");
            // Les 8 bits de poids faible doivent être exactement à 0
            assert_eq!(pcm_out[idx * 2] & 0xFF, 0);
            assert_eq!(pcm_out[idx * 2 + 1] & 0xFF, 0);
        }
    }
}
