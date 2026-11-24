use crate::AppState;
use crate::login_window;
use tauri::State;
use tidal_app_engine::activity::{ActivityPeriod, LastFmStatus};
use tidal_app_engine::BootstrapState;
use tidal_core::{AppPreferences, AudioDeviceDescriptor, ListeningActivityStats};
use tidal_platform_win::open_folder_in_explorer;

#[tauri::command]
#[specta::specta]
pub async fn update_preferences(
    preferences: AppPreferences,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .engine
        .update_preferences(preferences)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn refresh_devices(
    state: State<'_, AppState>,
) -> Result<Vec<AudioDeviceDescriptor>, String> {
    state
        .engine
        .refresh_devices()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn open_tidal_login(state: State<'_, AppState>) -> Result<Option<BootstrapState>, String> {
    let maybe_session = login_window::run_isolated_tidal_login().await?;
    match maybe_session {
        Some(session) => {
            let boot = state
                .engine
                .complete_web_login(session)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Some(boot))
        }
        None => Ok(None),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn logout(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.logout().await.map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn connect_lastfm(
    api_key: String,
    api_secret: String,
    username: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<LastFmStatus, String> {
    state
        .engine
        .connect_lastfm(api_key, api_secret, username, password)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn disconnect_lastfm(state: State<'_, AppState>) -> Result<LastFmStatus, String> {
    state
        .engine
        .disconnect_lastfm()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_activity_stats(
    period: ActivityPeriod,
    state: State<'_, AppState>,
) -> Result<ListeningActivityStats, String> {
    state
        .engine
        .get_activity_stats(period)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn clear_activity_history(state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .clear_activity_history()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn open_data_folder() -> Result<(), String> {
    let dir = tidal_cache::app_data_dir();
    open_folder_in_explorer(&dir).map_err(|e| e.to_string())
}
