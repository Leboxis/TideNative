use crate::AppState;
use crate::telemetry::spawn_telemetry_stream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::State;
use tidal_app_engine::BootstrapState;
use tidal_core::{RepeatMode, TrackSummary};

#[tauri::command]
#[specta::specta]
pub async fn get_bootstrap_state(state: State<'_, AppState>) -> Result<BootstrapState, String> {
    Ok(state.engine.bootstrap_state())
}

#[tauri::command]
pub fn subscribe_telemetry(
    channel: Channel<InvokeResponseBody>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let cancel = Arc::new(AtomicBool::new(false));
    if let Ok(mut guard) = state.telemetry_cancel.lock() {
        if let Some(prev) = guard.replace(Arc::clone(&cancel)) {
            prev.store(true, Ordering::Release);
        }
    }
    spawn_telemetry_stream(
        Arc::clone(&state.engine),
        Arc::clone(&state.window_visible),
        channel,
        cancel,
    );
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn play_tracks(
    tracks: Vec<TrackSummary>,
    start_index: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .engine
        .play_tracks(tracks, start_index as usize)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn play_queue_index(index: u32, state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .play_queue_index(index as usize)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn play_track_by_id(track_id: u64, state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .play_track_by_id(track_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_play_pause(state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .toggle_play_pause()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn next_track(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.next_track().await.map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn prev_track(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.prev_track().await.map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn seek_ms(position_ms: u64, state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .seek_ms(position_ms)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn set_volume(volume: f32, state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .set_volume(volume)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_mute(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.toggle_mute().await.map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn set_shuffle(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .set_shuffle(enabled)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn cycle_repeat(state: State<'_, AppState>) -> Result<RepeatMode, String> {
    state.engine.cycle_repeat().await.map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn queue_play_next(track: TrackSummary, state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .queue_play_next(track)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn queue_add(track: TrackSummary, state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .queue_add(track)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn queue_remove(index: u32, state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .queue_remove(index as usize)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn queue_move(
    from_index: u32,
    to_index: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .engine
        .queue_move(from_index as usize, to_index as usize)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn queue_clear(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.queue_clear().await.map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn play_track_radio(track_id: u64, state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .play_track_radio(track_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn play_artist_radio(artist_id: u64, state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .play_artist_radio(artist_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn play_album_radio(album_id: u64, state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .play_album_radio(album_id)
        .await
        .map_err(|e| e.to_string())
}
