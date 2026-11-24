//! Intégration **SMTC** (`SystemMediaTransportControls`) de Windows 10/11 :
//! - Touches multimédia clavier / casque (`Play`, `Pause`, `Next`, `Previous`, `Stop`)
//! - Affichage natif du titre, de l'artiste, de l'album et de la pochette dans le
//!   flyout Windows 11 (`Win+A` / OSD volume / écran de verrouillage).

use crate::{PlatformError, Result};
use tidal_core::{PlaybackState, TrackSummary};
use tokio::sync::mpsc;

/// Commande reçue depuis les touches multimédia Windows / casque Bluetooth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtcCommand {
    Play,
    Pause,
    Next,
    Previous,
    Stop,
}

#[cfg(windows)]
pub struct SmtcController {
    controls: windows::Media::SystemMediaTransportControls,
}

#[cfg(windows)]
impl SmtcController {
    /// Attache SMTC au `HWND` principal de la fenêtre `TideNative`.
    pub fn attach(hwnd_raw: isize, cmd_tx: mpsc::UnboundedSender<SmtcCommand>) -> Result<Self> {
        use std::ffi::c_void;
        use windows::Foundation::TypedEventHandler;
        use windows::Media::{
            SystemMediaTransportControls, SystemMediaTransportControlsButton,
            SystemMediaTransportControlsButtonPressedEventArgs,
        };
        use windows::Win32::Foundation::HWND;
        use windows::Win32::System::WinRT::{
            ISystemMediaTransportControlsInterop, RO_INIT_SINGLETHREADED, RoInitialize,
        };
        use windows::core::factory;

        if hwnd_raw == 0 {
            return Err(PlatformError::InvalidHandle);
        }

        // Initialise WinRT en STA (compatible avec `OleInitialize` du thread UI principal `winit`)
        let _ = unsafe { RoInitialize(RO_INIT_SINGLETHREADED) };

        let interop: ISystemMediaTransportControlsInterop =
            factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>()
                .map_err(|e| PlatformError::Smtc(e.message()))?;

        let hwnd = HWND(hwnd_raw as *mut c_void);
        let controls: SystemMediaTransportControls =
            unsafe { interop.GetForWindow(hwnd) }.map_err(|e| PlatformError::Smtc(e.message()))?;

        controls
            .SetIsEnabled(true)
            .map_err(|e| PlatformError::Smtc(e.message()))?;
        let _ = controls.SetIsPlayEnabled(true);
        let _ = controls.SetIsPauseEnabled(true);
        let _ = controls.SetIsNextEnabled(true);
        let _ = controls.SetIsPreviousEnabled(true);
        let _ = controls.SetIsStopEnabled(true);

        let handler = TypedEventHandler::<
            SystemMediaTransportControls,
            SystemMediaTransportControlsButtonPressedEventArgs,
        >::new(move |_sender, args| {
            if let Some(args) = args.as_ref() {
                if let Ok(btn) = args.Button() {
                    let mapped = match btn {
                        SystemMediaTransportControlsButton::Play => Some(SmtcCommand::Play),
                        SystemMediaTransportControlsButton::Pause => Some(SmtcCommand::Pause),
                        SystemMediaTransportControlsButton::Next => Some(SmtcCommand::Next),
                        SystemMediaTransportControlsButton::Previous => {
                            Some(SmtcCommand::Previous)
                        }
                        SystemMediaTransportControlsButton::Stop => Some(SmtcCommand::Stop),
                        _ => None,
                    };
                    if let Some(cmd) = mapped {
                        let _ = cmd_tx.send(cmd);
                    }
                }
            }
            Ok(())
        });

        controls
            .ButtonPressed(&handler)
            .map_err(|e| PlatformError::Smtc(e.message()))?;

        Ok(Self { controls })
    }

    /// Met à jour le statut Play / Pause / Stopped dans le flyout Windows.
    pub fn update_playback_state(&self, state: &PlaybackState) {
        use windows::Media::MediaPlaybackStatus;

        let status = match state {
            PlaybackState::Playing => MediaPlaybackStatus::Playing,
            PlaybackState::Paused => MediaPlaybackStatus::Paused,
            PlaybackState::Buffering => MediaPlaybackStatus::Changing,
            PlaybackState::Stopped | PlaybackState::Error(_) => MediaPlaybackStatus::Stopped,
        };
        let _ = self.controls.SetPlaybackStatus(status);
    }

    /// Met à jour les métadonnées (titre, artiste, album, pochette CDN) dans le flyout Windows 11.
    pub fn update_metadata(&self, track: &TrackSummary) {
        use windows::Foundation::Uri;
        use windows::Media::MediaPlaybackType;
        use windows::Storage::Streams::RandomAccessStreamReference;
        use windows::core::HSTRING;

        let Ok(updater) = self.controls.DisplayUpdater() else {
            return;
        };
        let _ = updater.SetType(MediaPlaybackType::Music);
        if let Ok(props) = updater.MusicProperties() {
            let _ = props.SetTitle(&HSTRING::from(&track.title));
            let _ = props.SetArtist(&HSTRING::from(&track.artist_name));
            let _ = props.SetAlbumTitle(&HSTRING::from(&track.album_title));
        }
        if let Some(cover_url) = track.cover_url(320) {
            if let Ok(uri) = Uri::CreateUri(&HSTRING::from(&cover_url)) {
                if let Ok(thumb) = RandomAccessStreamReference::CreateFromUri(&uri) {
                    let _ = updater.SetThumbnail(&thumb);
                }
            }
        }
        let _ = updater.Update();
    }
}

#[cfg(not(windows))]
pub struct SmtcController;

#[cfg(not(windows))]
impl SmtcController {
    pub fn attach(_hwnd_raw: isize, _cmd_tx: mpsc::UnboundedSender<SmtcCommand>) -> Result<Self> {
        Ok(Self)
    }

    pub fn update_playback_state(&self, _state: &PlaybackState) {}

    pub fn update_metadata(&self, _track: &TrackSummary) {}
}
