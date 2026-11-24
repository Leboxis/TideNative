//! Intégrations natives Windows 10/11 : DWM Mica & barre de titre sombre,
//! **SMTC** (`SystemMediaTransportControls`), presse-papiers Win32, ouverture
//! système via la crate `open` et fenêtre WebView2 dédiée à la connexion TIDAL.

pub mod dwm;
pub mod shell;
pub mod smtc;
pub mod tray;
pub mod webview_login;

pub use dwm::{DwmBackdropReport, apply_windows11_styling};
pub use shell::{
    open_folder_in_explorer, open_url_in_browser, read_clipboard_text, write_clipboard_text,
};
pub use smtc::{SmtcCommand, SmtcController};
pub use tray::{TrayCommand, TrayController, hide_window_to_tray, show_and_focus_window};
pub use webview_login::{WebTidalSession, WebViewLoginOutcome, run_tidal_web_login};

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("handle de fenêtre Windows (HWND) invalide")]
    InvalidHandle,

    #[error("erreur SMTC Windows : {0}")]
    Smtc(String),

    #[error("erreur d'icône de notification Windows : {0}")]
    Tray(String),

    #[error("erreur ouverture système (open) : {0}")]
    Shell(String),

    #[error("erreur WebView2 : {0}")]
    WebView(String),
}

pub type Result<T> = std::result::Result<T, PlatformError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_null_hwnd_cleanly() {
        assert!(apply_windows11_styling(0, true).is_err());
    }
}
