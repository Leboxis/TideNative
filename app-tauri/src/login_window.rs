use std::path::PathBuf;
use tidal_platform_win::{WebTidalSession, WebViewLoginOutcome, run_tidal_web_login};

pub fn isolated_login_profile_dir() -> PathBuf {
    tidal_core::paths::data_subdir("webview2-login")
        .unwrap_or_else(|_| tidal_cache::app_data_dir().join("webview2-login"))
}

pub async fn run_isolated_tidal_login() -> Result<Option<WebTidalSession>, String> {
    let profile_dir = isolated_login_profile_dir();
    let outcome = tokio::task::spawn_blocking(move || run_tidal_web_login(profile_dir))
        .await
        .map_err(|e| format!("tâche de connexion interrompue : {e}"))?
        .map_err(|e| e.to_string())?;

    match outcome {
        WebViewLoginOutcome::SessionCaptured(session) => Ok(Some(session)),
        WebViewLoginOutcome::Cancelled => Ok(None),
    }
}
