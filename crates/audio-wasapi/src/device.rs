//! Énumération des périphériques de sortie Windows Core Audio (`IMMDeviceEnumerator`)
//! et récupération du DAC WASAPI cible.

use crate::{Result, WasapiError};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tidal_core::AudioDeviceDescriptor;

#[cfg(windows)]
pub mod imp {
    use super::*;
    use windows::Win32::Foundation::{PROPERTYKEY, RPC_E_CHANGED_MODE};
    use windows::Win32::Media::Audio::{
        DEVICE_STATE_ACTIVE, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eMultimedia,
        eRender,
    };
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
        STGM_READ,
    };
    use windows::core::{GUID, HSTRING, PWSTR};

    /// `{a45c254e-df1c-4efd-8020-67d146a850e0}, pid = 14` (`PKEY_Device_FriendlyName`).
    const PKEY_DEVICE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
        fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
        pid: 14,
    };

    /// Initialise COM sur le thread courant s'il ne l'est pas déjà.
    pub fn ensure_com_initialized() -> Result<()> {
        // SAFETY: appel FFI standard Windows COM.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr.is_ok() || hr == RPC_E_CHANGED_MODE {
            Ok(())
        } else {
            Err(WasapiError::Com(hr.message()))
        }
    }

    pub fn create_enumerator() -> Result<IMMDeviceEnumerator> {
        ensure_com_initialized()?;
        // SAFETY: instanciation COM standard de MMDeviceEnumerator.
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .map_err(|e| WasapiError::Com(e.message()))
    }

    /// Liste tous les périphériques de sortie audio actifs sous Windows.
    ///
    /// S'exécute sur un thread court dédié (`COINIT_MULTITHREADED`) afin de ne
    /// **jamais** polluer l'appartement COM/OLE (STA) du thread UI principal (`winit`/`OleInitialize`).
    pub fn enumerate_output_devices() -> Result<Vec<AudioDeviceDescriptor>> {
        std::thread::Builder::new()
            .name("tidenative-wasapi-enum".into())
            .spawn(enumerate_output_devices_on_current_thread)
            .map_err(|e| WasapiError::Com(format!("spawn enum thread: {e}")))?
            .join()
            .map_err(|_| WasapiError::Com("thread d'énumération WASAPI interrompu".into()))?
    }

    fn enumerate_output_devices_on_current_thread() -> Result<Vec<AudioDeviceDescriptor>> {
        let enumerator = create_enumerator()?;

        let default_id = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) }
            .ok()
            .and_then(|dev| device_id(&dev).ok());

        let collection = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }
            .map_err(|e| WasapiError::Com(e.message()))?;

        let count = unsafe { collection.GetCount() }.map_err(|e| WasapiError::Com(e.message()))?;
        let mut devices = Vec::with_capacity(count as usize);

        for i in 0..count {
            if let Ok(dev) = unsafe { collection.Item(i) } {
                let id = device_id(&dev).unwrap_or_else(|_| format!("device-{i}"));
                let name = device_friendly_name(&dev).unwrap_or_else(|_| id.clone());
                let is_default = default_id.as_deref() == Some(id.as_str());
                devices.push(AudioDeviceDescriptor {
                    id,
                    name,
                    is_default,
                });
            }
        }

        Ok(devices)
    }

    /// Ouvre le périphérique WASAPI demandé (ou le périphérique multimédia par défaut si `None`).
    pub fn open_output_device(device_id_opt: Option<&str>) -> Result<(IMMDevice, AudioDeviceDescriptor)> {
        let enumerator = create_enumerator()?;
        let default_dev = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) }
            .map_err(|e| WasapiError::DeviceNotFound(e.message()))?;
        let default_id = device_id(&default_dev)?;

        let dev = match device_id_opt {
            Some(req_id) if !req_id.is_empty() => {
                let h = HSTRING::from(req_id);
                unsafe { enumerator.GetDevice(&h) }
                    .map_err(|e| WasapiError::DeviceNotFound(format!("{req_id}: {}", e.message())))?
            }
            _ => default_dev,
        };

        let id = device_id(&dev)?;
        let name = device_friendly_name(&dev).unwrap_or_else(|_| id.clone());
        let is_default = id == default_id;

        Ok((
            dev,
            AudioDeviceDescriptor {
                id,
                name,
                is_default,
            },
        ))
    }

    fn device_id(device: &IMMDevice) -> Result<String> {
        // SAFETY: GetId alloue une chaîne UTF-16 avec CoTaskMemAlloc.
        let pwstr = unsafe { device.GetId() }.map_err(|e| WasapiError::Com(e.message()))?;
        unsafe { pwstr_to_string_and_free(pwstr) }
    }

    fn device_friendly_name(device: &IMMDevice) -> Result<String> {
        // SAFETY: lecture du PropertyStore en STGM_READ.
        let store = unsafe { device.OpenPropertyStore(STGM_READ) }
            .map_err(|e| WasapiError::Com(e.message()))?;
        let prop = unsafe { store.GetValue(&PKEY_DEVICE_FRIENDLY_NAME) }
            .map_err(|e| WasapiError::Com(e.message()))?;
        let pwstr = unsafe { PropVariantToStringAlloc(&prop) }
            .map_err(|e| WasapiError::Com(e.message()))?;
        unsafe { pwstr_to_string_and_free(pwstr) }
    }

    unsafe fn pwstr_to_string_and_free(pwstr: PWSTR) -> Result<String> {
        if pwstr.is_null() {
            return Ok(String::new());
        }
        let s = unsafe { pwstr.to_string() }.unwrap_or_default();
        unsafe { CoTaskMemFree(Some(pwstr.0 as *const _)) };
        Ok(s)
    }
}

#[cfg(not(windows))]
pub mod imp {
    use super::*;
    pub fn enumerate_output_devices() -> Result<Vec<AudioDeviceDescriptor>> {
        Ok(Vec::new())
    }
}

pub use imp::enumerate_output_devices;

/// Scrutation du pas de veille : le thread de hot-plug vérifie le drapeau
/// d'arrêt toutes les 150 ms, ce qui borne le coût d'un `Drop` à 150 ms même
/// avec une période de scrutation longue (2 s).
const HOTPLUG_SLEEP_STEP: Duration = Duration::from_millis(150);

/// Surveille l'arrivée / le départ des DAC USB et casques en interrogeant
/// `IMMDeviceEnumerator` à intervalle régulier.
///
/// Le travail COM s'exécute sur un thread dédié (`"tidenative-wasapi-hotplug"`)
/// qui n'initialise jamais COM lui-même : `enumerate_output_devices` ouvre son
/// propre appartement MTA par appel, l'appartement STA du thread appelant
/// (UI `winit` / `OleInitialize`) reste donc intact.
pub struct AudioDeviceWatcher {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl AudioDeviceWatcher {
    /// Démarre la surveillance des périphériques de sortie.
    ///
    /// `on_change` est invoqué avec la nouvelle liste dès qu'elle diffère de la
    /// précédente (branchement / débranchement d'un DAC USB, changement de
    /// périphérique par défaut). La période est bornée à 150 ms minimum pour
    /// éviter une boucle de scrutation trop agressive.
    ///
    /// Si le thread de veille ne peut pas être créé (épuisement des ressources),
    /// le watcher reste inerte ; `Drop` ne joint alors aucun thread.
    pub fn start<F>(poll_interval: Duration, on_change: F) -> Self
    where
        F: Fn(Vec<AudioDeviceDescriptor>) + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = Arc::clone(&stop);
        let poll_interval = poll_interval.max(HOTPLUG_SLEEP_STEP);

        let handle = std::thread::Builder::new()
            .name("tidenative-wasapi-hotplug".into())
            .spawn(move || {
                let mut last_devices = enumerate_output_devices().unwrap_or_default();
                loop {
                    // Veille découpée : `Drop` peut joindre le thread en moins
                    // de 150 ms même si `poll_interval` vaut plusieurs secondes.
                    let mut slept = Duration::ZERO;
                    while slept < poll_interval {
                        if stop_worker.load(Ordering::Acquire) {
                            return;
                        }
                        let step = HOTPLUG_SLEEP_STEP.min(poll_interval - slept);
                        std::thread::sleep(step);
                        slept += step;
                    }
                    if stop_worker.load(Ordering::Acquire) {
                        return;
                    }
                    let devices = enumerate_output_devices().unwrap_or_default();
                    if devices != last_devices {
                        last_devices = devices.clone();
                        on_change(devices);
                    }
                }
            })
            .ok();

        Self { stop, handle }
    }
}

impl Drop for AudioDeviceWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
