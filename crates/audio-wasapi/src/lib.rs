//! Sortie audio Windows Core Audio **WASAPI Exclusif (Event-Driven)** Bit-Perfect
//! avec priorité thread **MMCSS `"Pro Audio"`** et ring buffer SPSC lock-free (`rtrb`).

pub mod device;
pub mod engine;
pub mod format;
pub mod ring;

pub use device::{AudioDeviceWatcher, enumerate_output_devices};
pub use engine::{
    WasapiActiveStream, WasapiClock, WasapiStreamConfig, live_audio_levels, set_realtime_volume,
    set_replay_gain,
};
pub use format::{
    HardwareSampleFormat, NegotiatedFormat, candidate_exclusive_formats, pack_msb_i32_to_hardware,
};
pub use ring::{PcmConsumer, PcmProducer, create_pcm_ring_buffer};

#[derive(Debug, thiserror::Error)]
pub enum WasapiError {
    #[error("erreur Windows COM / Core Audio : {0}")]
    Com(String),

    #[error("périphérique audio introuvable : {0}")]
    DeviceNotFound(String),

    #[error("format matériel refusé par le DAC en mode WASAPI Exclusif : {0}")]
    UnsupportedHardwareFormat(String),

    #[error("échec d'initialisation du flux WASAPI : {0}")]
    Initialization(String),
}

pub type Result<T> = std::result::Result<T, WasapiError>;

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn enumerates_windows_audio_endpoints_without_crashing() {
        let devices = enumerate_output_devices().expect("COM enumeration should succeed");
        // Sur une machine ou VM sans carte son branchée, la liste peut être vide mais l'appel COM doit réussir.
        for dev in &devices {
            assert!(!dev.id.is_empty());
            assert!(!dev.name.is_empty());
        }
    }

    #[test]
    fn enumeration_does_not_pollute_caller_thread_sta_apartment() {
        use windows::Win32::System::Com::{
            COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize,
        };

        // Simule un thread UI neuf (`winit` appelle `OleInitialize` qui exige STA)
        let handle = std::thread::spawn(|| {
            let _ = enumerate_output_devices().expect("enum must succeed");
            // Si `enumerate_output_devices` avait mis le thread courant en MTA,
            // CoInitializeEx(COINIT_APARTMENTTHREADED) échouerait avec RPC_E_CHANGED_MODE.
            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            assert!(
                hr.is_ok(),
                "caller thread COM apartment was polluted into MTA: {hr:?}"
            );
            unsafe { CoUninitialize() };
        });
        handle.join().unwrap();
    }

    #[test]
    fn device_watcher_starts_and_drops_cleanly_without_polluting_sta() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::{Duration, Instant};
        use windows::Win32::System::Com::{
            COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize,
        };

        // Même contrainte que le thread UI : démarrage puis `Drop` immédiat du
        // watcher ne doivent ni bloquer ni changer l'appartement COM courant.
        let handle = std::thread::spawn(|| {
            let changes = Arc::new(AtomicUsize::new(0));
            let changes_worker = Arc::clone(&changes);

            let started = Instant::now();
            let watcher = AudioDeviceWatcher::start(Duration::from_secs(2), move |_devices| {
                changes_worker.fetch_add(1, Ordering::Relaxed);
            });
            std::thread::sleep(Duration::from_millis(60));
            drop(watcher);

            assert!(
                started.elapsed() < Duration::from_secs(1),
                "Drop du watcher trop lent : {:?}",
                started.elapsed()
            );

            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            assert!(
                hr.is_ok(),
                "le watcher a pollué l'appartement COM du thread appelant : {hr:?}"
            );
            unsafe { CoUninitialize() };

            // Aucun changement de périphérique attendu en 60 ms : le callback
            // ne doit pas être invoqué sans différence réelle.
            assert_eq!(changes.load(Ordering::Relaxed), 0);
        });
        handle.join().unwrap();
    }
}
