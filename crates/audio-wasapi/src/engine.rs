//! Moteur de rendu WASAPI Exclusif Event-Driven + MMCSS `"Pro Audio"`.
//!
//! Fonctionnalités clés :
//! 1. **Négociation Bit-Perfect** (`IAudioClient::IsFormatSupported` en
//!    `AUDCLNT_SHAREMODE_EXCLUSIVE`) avec test prioritaire de `24-bit in 32-bit MSB`,
//!    puis `24-bit packed`, `32-bit` et `16-bit` à la fréquence exacte de la source
//!    (`44.1` .. `192 kHz`).
//! 2. **Alignement matériel du tampon** : gestion complète de
//!    `AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED` (`0x88890019`) requise par les pilotes
//!    USB Audio Class 2 et Intel HD Audio.
//! 3. **Boucle temps réel Event-Driven** (`WaitForSingleObject` sur `CreateEventW`)
//!    avec priorité système **MMCSS `"Pro Audio"`** et lecture directe depuis le
//!    ring buffer lock-free [`PcmConsumer`].

use crate::format::{
    HardwareSampleFormat, NegotiatedFormat, candidate_exclusive_formats, pack_msb_i32_to_hardware,
};
use crate::ring::PcmConsumer;
use crate::{Result, WasapiError};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use tidal_core::{PcmFormat, StreamFormatInfo};

/// Gain temps réel Q24 appliqué juste avant le transfert DMA WASAPI (latence < 10 ms).
static MASTER_GAIN_Q24: AtomicU32 = AtomicU32::new(1 << 24);
/// Gain de normalisation ReplayGain sans perte (Q24, `1 << 24` = 0 dB, protégé contre tout écrêtage).
static REPLAY_GAIN_Q24: AtomicU32 = AtomicU32::new(1 << 24);
static MASTER_BIT_PERFECT_LOCK: AtomicBool = AtomicBool::new(false);
/// Niveaux temps réel de 5 bandes de fréquences (stockés en `f32::to_bits()`, plage `0.0..=1.0`).
static LIVE_AUDIO_BANDS: [AtomicU32; 5] = [
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
];

/// Met à jour instantanément le volume temps réel (courbe perceptuelle douce + dithering TPDF 24-bit).
/// Lorsque `bit_perfect_lock == true` ou `linear >= 0.999` (et ReplayGain inactif), le signal reste 100 % bit-exact.
pub fn set_realtime_volume(linear: f32, bit_perfect_lock: bool) {
    MASTER_BIT_PERFECT_LOCK.store(bit_perfect_lock, Ordering::Release);
    let clamped = linear.clamp(0.0, 1.0);
    // Courbe perceptuelle quadratique (douce et naturelle comme macOS), 1.0 exact à fond.
    let perceptual = if clamped >= 0.999 {
        1.0
    } else if clamped <= 0.001 {
        0.0
    } else {
        clamped * clamped
    };
    let q24 = ((perceptual as f64) * ((1u32 << 24) as f64)).round() as u32;
    MASTER_GAIN_Q24.store(q24.min(1 << 24), Ordering::Release);
}

/// Gain de repli ReplayGain 2.0 appliqué aux flux dépourvus de métadonnées `trackReplayGain`
/// lorsque la normalisation de volume est activée, afin d'éviter un saut de +8 à +10 dB
/// entre les codecs/pistes analysés (-18 LUFS) et les flux non tagués.
const DEFAULT_UNTAGGED_REPLAY_GAIN_DB: f32 = -6.0;

/// Applique la normalisation de volume sans perte (ReplayGain TIDAL) :
/// simple multiplication linéaire 64-bit avec protection anti-écrêtage (`peak <= 0 dBFS`),
/// sans aucune compression dynamique ni égalisation.
pub fn set_replay_gain(gain_db: Option<f32>, peak_amplitude: Option<f32>, enabled: bool) {
    const UNITY: u32 = 1 << 24;
    if !enabled {
        REPLAY_GAIN_Q24.store(UNITY, Ordering::Release);
        return;
    }
    let db = gain_db
        .filter(|v| v.is_finite())
        .unwrap_or(DEFAULT_UNTAGGED_REPLAY_GAIN_DB);
    // Conversion dB -> linéaire en double précision
    let mut linear = 10.0_f64.powf(f64::from(db) / 20.0);
    // Protection stricte contre tout écrêtage numérique (true-peak headroom <= 0 dBFS)
    if let Some(peak) = peak_amplitude.filter(|p| p.is_finite() && *p > 0.001) {
        let max_safe = 0.999 / f64::from(peak);
        if linear > max_safe {
            linear = max_safe;
        }
    }
    // Borne de sécurité (-24 dB .. +6 dB)
    linear = linear.clamp(0.0625, 2.0);
    let q24 = (linear * f64::from(UNITY)).round() as u32;
    REPLAY_GAIN_Q24.store(q24.clamp(1, UNITY * 2), Ordering::Release);
}

/// Retourne les 5 niveaux temps réel (`0.0..=1.0`) calculés directement sur le flux audio joué.
pub fn live_audio_levels() -> [f32; 5] {
    [
        f32::from_bits(LIVE_AUDIO_BANDS[0].load(Ordering::Relaxed)),
        f32::from_bits(LIVE_AUDIO_BANDS[1].load(Ordering::Relaxed)),
        f32::from_bits(LIVE_AUDIO_BANDS[2].load(Ordering::Relaxed)),
        f32::from_bits(LIVE_AUDIO_BANDS[3].load(Ordering::Relaxed)),
        f32::from_bits(LIVE_AUDIO_BANDS[4].load(Ordering::Relaxed)),
    ]
}

fn clear_live_audio_levels() {
    for slot in &LIVE_AUDIO_BANDS {
        slot.store(0, Ordering::Relaxed);
    }
}

/// Configuration d'ouverture d'un flux de sortie WASAPI.
#[derive(Debug, Clone)]
pub struct WasapiStreamConfig {
    /// Identifiant WASAPI du DAC (`None` = périphérique par défaut).
    pub device_id: Option<String>,
    /// `true` = `AUDCLNT_SHAREMODE_EXCLUSIVE` (Bit-Perfect, horloge matérielle directe).
    /// `false` = `AUDCLNT_SHAREMODE_SHARED` (mixeur Windows).
    pub exclusive: bool,
    /// Si `true` et que le DAC refuse le mode Exclusif (format non supporté ou périphérique
    /// occupé), bascule proprement en `AUDCLNT_SHAREMODE_SHARED` au lieu d'échouer.
    pub fallback_to_shared: bool,
    /// Durée cible du tampon matériel WASAPI en millisecondes (ex. `20` ms).
    pub target_buffer_ms: u32,
}

impl Default for WasapiStreamConfig {
    fn default() -> Self {
        Self {
            device_id: None,
            exclusive: true,
            fallback_to_shared: true,
            target_buffer_ms: 20,
        }
    }
}

/// Poignée légère et partageable vers l'horloge matérielle et le compteur d'xruns d'un flux WASAPI.
#[derive(Debug, Clone)]
pub struct WasapiClock {
    sample_rate: u32,
    frames_rendered: Arc<AtomicU64>,
    underruns: Arc<AtomicU64>,
}

impl WasapiClock {
    /// Nombre total de trames PCM envoyées au tampon DMA du DAC.
    pub fn frames_rendered(&self) -> u64 {
        self.frames_rendered.load(Ordering::Relaxed)
    }

    /// Position courante en millisecondes mesurée sur l'horloge matérielle du DAC.
    pub fn rendered_position_ms(&self) -> u64 {
        let frames = self.frames_rendered();
        let rate = u64::from(self.sample_rate.max(1));
        (frames * 1000) / rate
    }

    /// Nombre d'underruns (famines tampon / xruns) détectés depuis le démarrage du flux.
    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }
}

/// Contrôleur d'un flux WASAPI actif tournant sur son thread temps réel dédié.
pub struct WasapiActiveStream {
    negotiated: NegotiatedFormat,
    format_info: StreamFormatInfo,
    paused: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
    flush_ring: Arc<AtomicBool>,
    frames_rendered: Arc<AtomicU64>,
    underruns: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}

impl WasapiActiveStream {
    /// Démarre un flux de sortie WASAPI pour le format `source_pcm` (`codec_label` = `"FLAC"`, etc.)
    /// alimenté en temps réel par le ring buffer lock-free `consumer`.
    #[cfg(windows)]
    pub fn start(
        config: WasapiStreamConfig,
        source_pcm: PcmFormat,
        codec_label: impl Into<String>,
        consumer: PcmConsumer,
    ) -> Result<Self> {
        win_engine::spawn_wasapi_stream(config, source_pcm, codec_label.into(), consumer)
    }

    pub fn negotiated_format(&self) -> NegotiatedFormat {
        self.negotiated
    }

    pub fn stream_format_info(&self) -> &StreamFormatInfo {
        &self.format_info
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::Release);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Release);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    /// Nombre total de trames PCM effectivement envoyées au DAC depuis l'ouverture du flux.
    pub fn frames_rendered(&self) -> u64 {
        self.frames_rendered.load(Ordering::Relaxed)
    }

    /// Position courante de lecture en millisecondes calculée depuis l'horloge matérielle du DAC.
    pub fn rendered_position_ms(&self) -> u64 {
        let frames = self.frames_rendered();
        let rate = u64::from(self.negotiated.sample_rate.max(1));
        (frames * 1000) / rate
    }

    /// Nombre de famines tampon (*buffer underruns* / *xruns*) détectées.
    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    /// Poignée partageable vers l'horloge matérielle et les compteurs d'underruns du DAC.
    pub fn clock(&self) -> WasapiClock {
        WasapiClock {
            sample_rate: self.negotiated.sample_rate,
            frames_rendered: Arc::clone(&self.frames_rendered),
            underruns: Arc::clone(&self.underruns),
        }
    }

    /// Poignée atomique permettant au thread décodeur de vider instantanément le ring buffer
    /// lors d'un seek (< 10 ms) sans fermer le flux matériel WASAPI.
    pub fn flush_ring_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.flush_ring)
    }

    /// Arrête proprement le thread temps réel WASAPI et libère le verrou exclusif sur le DAC.
    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for WasapiActiveStream {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(windows)]
mod win_engine {
    use super::*;
    use crate::device::imp::open_output_device;
    use std::sync::mpsc;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
    use windows::Win32::Media::Audio::{
        AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED,
        AUDCLNT_E_DEVICE_INVALIDATED, AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
        AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, IAudioClient, IAudioRenderClient, IMMDevice,
        WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0,
    };
    use windows::Win32::System::Com::CLSCTX_ALL;
    use windows::Win32::System::Threading::{
        AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW,
        WaitForSingleObject,
    };
    use windows::core::{GUID, PCWSTR};

    const WAVE_FORMAT_EXTENSIBLE_TAG: u16 = 0xFFFE;
    const SPEAKER_FRONT_LEFT: u32 = 0x1;
    const SPEAKER_FRONT_RIGHT: u32 = 0x2;
    const SPEAKER_FRONT_CENTER: u32 = 0x4;

    /// `KSDATAFORMAT_SUBTYPE_PCM` (`00000001-0000-0010-8000-00aa00389b71`)
    const SUBTYPE_PCM: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
    /// `KSDATAFORMAT_SUBTYPE_IEEE_FLOAT` (`00000003-0000-0010-8000-00aa00389b71`)
    const SUBTYPE_IEEE_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

    struct MmcssGuard(Option<HANDLE>);
    impl MmcssGuard {
        fn enter_pro_audio() -> Self {
            let mut task_idx: u32 = 0;
            let name: [u16; 10] = [
                b'P' as u16,
                b'r' as u16,
                b'o' as u16,
                b' ' as u16,
                b'A' as u16,
                b'u' as u16,
                b'd' as u16,
                b'i' as u16,
                b'o' as u16,
                0,
            ];
            // SAFETY: enregistre le thread courant auprès du planificateur multimédia Windows (avrt.dll).
            let handle =
                unsafe { AvSetMmThreadCharacteristicsW(PCWSTR(name.as_ptr()), &mut task_idx) }.ok();
            Self(handle)
        }
    }
    impl Drop for MmcssGuard {
        fn drop(&mut self) {
            if let Some(h) = self.0.take() {
                unsafe {
                    let _ = AvRevertMmThreadCharacteristics(h);
                }
            }
        }
    }

    pub fn spawn_wasapi_stream(
        config: WasapiStreamConfig,
        source_pcm: PcmFormat,
        codec_label: String,
        mut consumer: PcmConsumer,
    ) -> Result<WasapiActiveStream> {
        let paused = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let flush_ring = Arc::new(AtomicBool::new(false));
        let frames_rendered = Arc::new(AtomicU64::new(0));
        let underruns = Arc::new(AtomicU64::new(0));

        let paused_cl = Arc::clone(&paused);
        let stop_cl = Arc::clone(&stop_flag);
        let flush_cl = Arc::clone(&flush_ring);
        let frames_cl = Arc::clone(&frames_rendered);
        let underruns_cl = Arc::clone(&underruns);

        let (init_tx, init_rx) = mpsc::sync_channel::<Result<(NegotiatedFormat, StreamFormatInfo)>>(1);

        let worker = thread::Builder::new()
            .name("tidenative-wasapi-rt".into())
            .spawn(move || {
                let _mmcss = MmcssGuard::enter_pro_audio();

                let setup = init_and_negotiate(&config, source_pcm, &codec_label);
                let (audio_client, render_client, event_handle, negotiated, _info) = match setup {
                    Ok(bundle) => {
                        let _ = init_tx.send(Ok((bundle.3, bundle.4.clone())));
                        bundle
                    }
                    Err(e) => {
                        let _ = init_tx.send(Err(e));
                        return;
                    }
                };

                run_event_driven_loop(
                    audio_client,
                    render_client,
                    event_handle,
                    negotiated,
                    &mut consumer,
                    &paused_cl,
                    &stop_cl,
                    &flush_cl,
                    &frames_cl,
                    &underruns_cl,
                );
            })
            .map_err(|e| WasapiError::Initialization(format!("spawn audio thread: {e}")))?;

        match init_rx.recv() {
            Ok(Ok((negotiated, format_info))) => Ok(WasapiActiveStream {
                negotiated,
                format_info,
                paused,
                stop_flag,
                flush_ring,
                frames_rendered,
                underruns,
                worker: Some(worker),
            }),
            Ok(Err(e)) => {
                let _ = worker.join();
                Err(e)
            }
            Err(e) => {
                let _ = worker.join();
                Err(WasapiError::Initialization(format!(
                    "thread WASAPI terminé prématurément : {e}"
                )))
            }
        }
    }

    fn build_waveformatextensible(
        sample_rate: u32,
        channels: u16,
        hw_fmt: HardwareSampleFormat,
    ) -> WAVEFORMATEXTENSIBLE {
        let bytes_per_sample = hw_fmt.bytes_per_sample() as u16;
        let block_align = channels * bytes_per_sample;
        let avg_bytes_per_sec = sample_rate * u32::from(block_align);
        let channel_mask = match channels {
            1 => SPEAKER_FRONT_CENTER,
            _ => SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT,
        };
        let sub_format = match hw_fmt {
            HardwareSampleFormat::F32 => SUBTYPE_IEEE_FLOAT,
            _ => SUBTYPE_PCM,
        };

        WAVEFORMATEXTENSIBLE {
            Format: WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_EXTENSIBLE_TAG,
                nChannels: channels,
                nSamplesPerSec: sample_rate,
                nAvgBytesPerSec: avg_bytes_per_sec,
                nBlockAlign: block_align,
                wBitsPerSample: hw_fmt.container_bits(),
                cbSize: 22,
            },
            Samples: WAVEFORMATEXTENSIBLE_0 {
                wValidBitsPerSample: hw_fmt.valid_bits(),
            },
            dwChannelMask: channel_mask,
            SubFormat: sub_format,
        }
    }

    fn init_and_negotiate(
        config: &WasapiStreamConfig,
        source_pcm: PcmFormat,
        codec_label: &str,
    ) -> Result<(
        IAudioClient,
        IAudioRenderClient,
        HANDLE,
        NegotiatedFormat,
        StreamFormatInfo,
    )> {
        let (device, descriptor) = open_output_device(config.device_id.as_deref())?;

        if config.exclusive {
            match init_exclusive(&device, &descriptor.name, config, source_pcm, codec_label) {
                Ok(bundle) => Ok(bundle),
                Err(_) if config.fallback_to_shared => {
                    init_shared(&device, &descriptor.name, config, source_pcm, codec_label)
                }
                Err(e) => Err(e),
            }
        } else {
            init_shared(&device, &descriptor.name, config, source_pcm, codec_label)
        }
    }

    fn init_exclusive(
        device: &IMMDevice,
        device_name: &str,
        config: &WasapiStreamConfig,
        source_pcm: PcmFormat,
        codec_label: &str,
    ) -> Result<(
        IAudioClient,
        IAudioRenderClient,
        HANDLE,
        NegotiatedFormat,
        StreamFormatInfo,
    )> {
        let mut audio_client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }
            .map_err(|e| WasapiError::Initialization(e.message()))?;

        // 1. Si le repli vers WASAPI Partagé est activé, vérifier que le périphérique est
        //    un vrai DAC multi-horloges supportant les deux familles de fréquences (44,1 kHz ET 48 kHz)
        //    en mode WASAPI Exclusif. Les casques USB à horloge fixe 48 kHz (qui refusent 44,1 kHz
        //    en Exclusif) basculeraient sinon entre WASAPI Partagé (volume Windows + APO) sur les
        //    pistes 44,1 kHz et WASAPI Exclusif (volume matériel 100 % contournant le mixeur Windows)
        //    sur les pistes 48 kHz, provoquant un écart brutal de volume selon le format PCM.
        let candidates = candidate_exclusive_formats(source_pcm);
        if config.fallback_to_shared {
            let complementary_rate = if source_pcm.sample_rate % 44_100 == 0 {
                48_000
            } else {
                44_100
            };
            let supports_complementary = candidates.iter().any(|&cand| {
                let wf_comp =
                    build_waveformatextensible(complementary_rate, source_pcm.channels, cand);
                unsafe {
                    audio_client
                        .IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, &wf_comp.Format, None)
                        .is_ok()
                }
            });
            if !supports_complementary {
                return Err(WasapiError::UnsupportedHardwareFormat(format!(
                    "le périphérique « {device_name} » est à horloge fixe (ne supporte pas {complementary_rate} Hz en mode Exclusif) : utilisation homogène de WASAPI Partagé"
                )));
            }
        }

        // 2. Négocier le conteneur PCM supporté matériellement à la fréquence exacte de la source
        let mut chosen: Option<(HardwareSampleFormat, WAVEFORMATEXTENSIBLE)> = None;

        for cand in candidates {
            let wf = build_waveformatextensible(source_pcm.sample_rate, source_pcm.channels, cand);
            let hr = unsafe {
                audio_client.IsFormatSupported(
                    AUDCLNT_SHAREMODE_EXCLUSIVE,
                    &wf.Format,
                    None,
                )
            };
            if hr.is_ok() {
                chosen = Some((cand, wf));
                break;
            }
        }

        let (hw_fmt, wf) = chosen.ok_or_else(|| {
            WasapiError::UnsupportedHardwareFormat(format!(
                "le DAC « {device_name} » a refusé {} Hz ({}-bit) en mode WASAPI Exclusif",
                source_pcm.sample_rate, source_pcm.bits_per_sample
            ))
        })?;

        // 2. Calculer la période matérielle en unités de 100 ns (HNS)
        let mut default_period: i64 = 0;
        let mut min_period: i64 = 0;
        unsafe { audio_client.GetDevicePeriod(Some(&mut default_period), Some(&mut min_period)) }
            .map_err(|e| WasapiError::Initialization(e.message()))?;

        let requested_hns = (i64::from(config.target_buffer_ms.max(10)) * 10_000).max(default_period);

        // 3. Initialiser IAudioClient avec gestion de AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED
        let init_res = unsafe {
            audio_client.Initialize(
                AUDCLNT_SHAREMODE_EXCLUSIVE,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                requested_hns,
                requested_hns,
                &wf.Format,
                None,
            )
        };

        if let Err(err) = init_res {
            if err.code() == AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED {
                // Danse officielle Microsoft Core Audio pour réaligner le tampon DMA
                let aligned_frames = unsafe { audio_client.GetBufferSize() }
                    .map_err(|e| WasapiError::Initialization(e.message()))?;
                let aligned_hns = ((10_000_000.0 * f64::from(aligned_frames)
                    / f64::from(source_pcm.sample_rate))
                    + 0.5) as i64;

                audio_client = unsafe { device.Activate(CLSCTX_ALL, None) }
                    .map_err(|e| WasapiError::Initialization(e.message()))?;

                unsafe {
                    audio_client.Initialize(
                        AUDCLNT_SHAREMODE_EXCLUSIVE,
                        AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                        aligned_hns,
                        aligned_hns,
                        &wf.Format,
                        None,
                    )
                }
                .map_err(|e| {
                    WasapiError::Initialization(format!(
                        "échec Initialize après alignement ({aligned_frames} trames) : {}",
                        e.message()
                    ))
                })?;
            } else {
                return Err(WasapiError::Initialization(format!(
                    "WASAPI Exclusif Initialize refusé sur « {device_name} » ({:#010x}) : {}",
                    err.code().0 as u32,
                    err.message()
                )));
            }
        }

        finalize_client(
            audio_client,
            source_pcm,
            hw_fmt,
            true,
            device_name,
            codec_label,
        )
    }

    fn init_shared(
        device: &IMMDevice,
        device_name: &str,
        config: &WasapiStreamConfig,
        source_pcm: PcmFormat,
        codec_label: &str,
    ) -> Result<(
        IAudioClient,
        IAudioRenderClient,
        HANDLE,
        NegotiatedFormat,
        StreamFormatInfo,
    )> {
        let audio_client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }
            .map_err(|e| WasapiError::Initialization(e.message()))?;

        let hw_fmt = HardwareSampleFormat::F32;
        let wf = build_waveformatextensible(source_pcm.sample_rate, source_pcm.channels, hw_fmt);
        let requested_hns = i64::from(config.target_buffer_ms.max(30)) * 10_000;
        let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK
            | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
            | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;

        unsafe {
            audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                flags,
                requested_hns,
                0,
                &wf.Format,
                None,
            )
        }
        .map_err(|e| WasapiError::Initialization(e.message()))?;

        finalize_client(
            audio_client,
            source_pcm,
            hw_fmt,
            false,
            device_name,
            codec_label,
        )
    }

    fn finalize_client(
        audio_client: IAudioClient,
        source_pcm: PcmFormat,
        hw_fmt: HardwareSampleFormat,
        is_exclusive: bool,
        device_name: &str,
        codec_label: &str,
    ) -> Result<(
        IAudioClient,
        IAudioRenderClient,
        HANDLE,
        NegotiatedFormat,
        StreamFormatInfo,
    )> {
        let event_handle = unsafe { CreateEventW(None, false, false, None) }
            .map_err(|e| WasapiError::Initialization(e.message()))?;

        if let Err(e) = unsafe { audio_client.SetEventHandle(event_handle) } {
            let _ = unsafe { CloseHandle(event_handle) };
            return Err(WasapiError::Initialization(e.message()));
        }

        let buffer_frames = unsafe { audio_client.GetBufferSize() }
            .map_err(|e| WasapiError::Initialization(e.message()))?;

        let render_client: IAudioRenderClient = unsafe { audio_client.GetService() }
            .map_err(|e| WasapiError::Initialization(e.message()))?;

        let negotiated = NegotiatedFormat {
            sample_rate: source_pcm.sample_rate,
            channels: source_pcm.channels,
            sample_format: hw_fmt,
            is_exclusive,
            buffer_frames,
        };

        let info = StreamFormatInfo {
            codec: codec_label.to_string(),
            source_pcm,
            dac_container_bits: hw_fmt.container_bits(),
            dac_valid_bits: hw_fmt.valid_bits(),
            is_exclusive,
            is_bit_perfect: is_exclusive && hw_fmt.is_bit_perfect_for(source_pcm.bits_per_sample),
            device_name: device_name.to_string(),
        };

        Ok((audio_client, render_client, event_handle, negotiated, info))
    }

    #[allow(clippy::too_many_arguments)]
    fn run_event_driven_loop(
        audio_client: IAudioClient,
        render_client: IAudioRenderClient,
        event_handle: HANDLE,
        negotiated: NegotiatedFormat,
        consumer: &mut PcmConsumer,
        paused: &AtomicBool,
        stop_flag: &AtomicBool,
        flush_ring: &AtomicBool,
        frames_rendered: &AtomicU64,
        underruns: &AtomicU64,
    ) {
        let channels = usize::from(negotiated.channels.max(1));
        let max_frames = negotiated.buffer_frames as usize;
        // Tampon de travail pré-alloué une seule fois hors de la boucle temps réel
        let mut scratch_i32 = vec![0i32; max_frames * channels];
        let mut current_gain_q24 = effective_target_gain_q24();
        let mut prng_state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut meter_lp = [0.0f32; 4];

        // Pré-amorçage initial du tampon DMA avant `Start()` pour éviter tout clic à l'allumage
        fill_hardware_buffer(
            &render_client,
            negotiated.buffer_frames,
            negotiated,
            consumer,
            &mut scratch_i32,
            paused.load(Ordering::Acquire),
            frames_rendered,
            underruns,
            &mut current_gain_q24,
            &mut prng_state,
            &mut meter_lp,
        );

        if unsafe { audio_client.Start() }.is_err() {
            clear_live_audio_levels();
            let _ = unsafe { CloseHandle(event_handle) };
            return;
        }

        while !stop_flag.load(Ordering::Acquire) {
            let wait_res = unsafe { WaitForSingleObject(event_handle, 15) };
            if stop_flag.load(Ordering::Acquire) {
                break;
            }
            // Vidage instantané du ring buffer sur seek (< 15 ms)
            if flush_ring.swap(false, Ordering::AcqRel) {
                let avail = consumer.slots();
                if avail > 0 {
                    if let Ok(chunk) = consumer.read_chunk(avail) {
                        chunk.commit_all();
                    }
                }
            }
            if wait_res != WAIT_OBJECT_0 {
                continue;
            }

            let frames_to_write = if negotiated.is_exclusive {
                negotiated.buffer_frames
            } else {
                let padding = unsafe { audio_client.GetCurrentPadding() }.unwrap_or(0);
                negotiated.buffer_frames.saturating_sub(padding)
            };

            if frames_to_write == 0 {
                continue;
            }

            if !fill_hardware_buffer(
                &render_client,
                frames_to_write,
                negotiated,
                consumer,
                &mut scratch_i32,
                paused.load(Ordering::Acquire),
                frames_rendered,
                underruns,
                &mut current_gain_q24,
                &mut prng_state,
                &mut meter_lp,
            ) {
                break;
            }
        }

        clear_live_audio_levels();
        let _ = unsafe { audio_client.Stop() };
        let _ = unsafe { CloseHandle(event_handle) };
    }

    /// Renvoie `false` uniquement si le périphérique audio a été débranché (`AUDCLNT_E_DEVICE_INVALIDATED`).
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn fill_hardware_buffer(
        render_client: &IAudioRenderClient,
        frames_to_write: u32,
        negotiated: NegotiatedFormat,
        consumer: &mut PcmConsumer,
        scratch_i32: &mut [i32],
        is_paused: bool,
        frames_rendered: &AtomicU64,
        underruns: &AtomicU64,
        current_gain_q24: &mut f64,
        prng_state: &mut u64,
        meter_lp: &mut [f32; 4],
    ) -> bool {
        let dma_ptr = match unsafe { render_client.GetBuffer(frames_to_write) } {
            Ok(ptr) => ptr,
            Err(e) if e.code() == AUDCLNT_E_DEVICE_INVALIDATED => return false,
            Err(_) => return true,
        };

        let channels = usize::from(negotiated.channels.max(1));
        let needed_samples = (frames_to_write as usize) * channels;
        let available_samples = consumer.slots();

        if is_paused || available_samples == 0 {
            clear_live_audio_levels();
            if !is_paused && frames_rendered.load(Ordering::Relaxed) > 0 {
                underruns.fetch_add(1, Ordering::Relaxed);
            }
            let _ = unsafe {
                render_client.ReleaseBuffer(frames_to_write, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)
            };
            return true;
        }

        let to_read = needed_samples.min(available_samples).min(scratch_i32.len());
        let mut read_count = 0;

        if let Ok(chunk) = consumer.read_chunk(to_read) {
            let (s1, s2) = chunk.as_slices();
            let l1 = s1.len();
            scratch_i32[..l1].copy_from_slice(s1);
            scratch_i32[l1..l1 + s2.len()].copy_from_slice(s2);
            read_count = l1 + s2.len();
            chunk.commit_all();
        }

        if read_count < needed_samples {
            scratch_i32[read_count..needed_samples].fill(0);
            if frames_rendered.load(Ordering::Relaxed) > 0 {
                underruns.fetch_add(1, Ordering::Relaxed);
            }
        }

        // 1. Calcul temps réel des 5 bandes d'énergie audio (pré-gain, zéro allocation)
        update_live_bands(&scratch_i32[..needed_samples], channels, meter_lp);

        // 2. Volume temps réel ultra-réactif (rampe anti-clic + dithering TPDF 24-bit, bypass exact à 100 %)
        apply_realtime_gain_ramp(
            &mut scratch_i32[..needed_samples],
            current_gain_q24,
            prng_state,
        );

        let dma_byte_len = (frames_to_write as usize) * negotiated.bytes_per_frame();
        // SAFETY: `dma_ptr` pointe vers un tampon DMA de `frames_to_write` trames alloué par WASAPI.
        let dma_slice = unsafe { std::slice::from_raw_parts_mut(dma_ptr, dma_byte_len) };

        pack_msb_i32_to_hardware(
            &scratch_i32[..needed_samples],
            dma_slice,
            negotiated.sample_format,
        );

        let _ = unsafe { render_client.ReleaseBuffer(frames_to_write, 0) };
        frames_rendered.fetch_add((read_count / channels) as u64, Ordering::Relaxed);
        true
    }

    #[inline]
    fn update_live_bands(samples: &[i32], channels: usize, lp: &mut [f32; 4]) {
        if samples.is_empty() || channels == 0 {
            return;
        }
        const INV_I32: f32 = 1.0 / (i32::MAX as f32);
        // 4 filtres passe-bas 1-pôle en cascade -> 5 bandes (graves -> aigus)
        let mut energy = [0.0f32; 5];
        let mut frames = 0u32;
        let step = channels * 2; // sous-échantillonnage x2 pour coût CPU négligeable
        let mut i = 0;
        while i < samples.len() {
            let x = (samples[i] as f32) * INV_I32;
            lp[0] += 0.015 * (x - lp[0]); // Sub / Bass
            lp[1] += 0.055 * (x - lp[1]); // Low-Mid
            lp[2] += 0.160 * (x - lp[2]); // Mid
            lp[3] += 0.380 * (x - lp[3]); // High-Mid

            let b0 = lp[0];
            let b1 = lp[1] - lp[0];
            let b2 = lp[2] - lp[1];
            let b3 = lp[3] - lp[2];
            let b4 = x - lp[3];

            energy[0] += b0 * b0;
            energy[1] += b1 * b1;
            energy[2] += b2 * b2;
            energy[3] += b3 * b3;
            energy[4] += b4 * b4;
            frames += 1;
            i += step;
        }
        if frames == 0 {
            return;
        }
        let inv_n = 1.0 / (frames as f32);
        let boosts = [2.6f32, 3.2, 3.6, 4.4, 5.2];
        for idx in 0..5 {
            let rms = (energy[idx] * inv_n).sqrt();
            let norm = (rms * boosts[idx]).powf(0.65).clamp(0.0, 1.0);
            LIVE_AUDIO_BANDS[idx].store(norm.to_bits(), Ordering::Relaxed);
        }
    }

    #[inline]
    fn effective_target_gain_q24() -> f64 {
        const UNITY: f64 = (1u32 << 24) as f64;
        let locked = MASTER_BIT_PERFECT_LOCK.load(Ordering::Acquire);
        let master = if locked {
            UNITY
        } else {
            f64::from(MASTER_GAIN_Q24.load(Ordering::Acquire))
        };
        let rg = f64::from(REPLAY_GAIN_Q24.load(Ordering::Acquire)) / UNITY;
        (master * rg).clamp(0.0, UNITY * 2.0)
    }

    #[inline]
    fn apply_realtime_gain_ramp(
        samples: &mut [i32],
        current_gain_q24: &mut f64,
        prng_state: &mut u64,
    ) {
        const UNITY: f64 = (1u32 << 24) as f64;
        let target_q24 = effective_target_gain_q24();

        if (target_q24 - UNITY).abs() < 1.0 && (*current_gain_q24 - UNITY).abs() < 1.0 {
            *current_gain_q24 = UNITY;
            return; // 100 % bit-exact (gain 0 dB exact)
        }

        let len = samples.len().max(1) as f64;
        let step = (target_q24 - *current_gain_q24) / len;
        let mut g = *current_gain_q24;

        for s in samples.iter_mut() {
            g += step;
            let q24 = (g.round() as i64).clamp(0, 1 << 25);
            let scaled = (i64::from(*s) * q24) >> 24;
            let r1 = xorshift64(prng_state) & 0xFF;
            let r2 = xorshift64(prng_state) & 0xFF;
            let dither = (r1 as i64) - (r2 as i64);
            *s = (scaled + dither).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        }
        *current_gain_q24 = target_q24;
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
}
