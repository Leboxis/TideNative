use crate::AppState;
use tauri::State;
use tidal_app_engine::{
    LibrarySyncPayload, PlaylistDetailData, PlaylistExportFormat, PlaylistExportPayload,
    PlaylistImportResult,
};
use tidal_core::{AlbumSummary, ArtistSummary, PlaylistSummary, TrackSummary};

#[tauri::command]
#[specta::specta]
pub async fn refresh_library(state: State<'_, AppState>) -> Result<LibrarySyncPayload, String> {
    state
        .engine
        .refresh_library()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn refresh_discover(state: State<'_, AppState>) -> Result<Vec<TrackSummary>, String> {
    state
        .engine
        .refresh_discover()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_favorite_track(
    track: TrackSummary,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    state
        .engine
        .toggle_favorite_track(track)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_favorite_album(
    album_id: u64,
    album: Option<AlbumSummary>,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    state
        .engine
        .toggle_favorite_album(album_id, album)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_favorite_artist(
    artist_id: u64,
    artist: Option<ArtistSummary>,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    state
        .engine
        .toggle_favorite_artist(artist_id, artist)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_playlist_tracks(
    playlist_uuid: String,
    state: State<'_, AppState>,
) -> Result<PlaylistDetailData, String> {
    state
        .engine
        .get_playlist_tracks(playlist_uuid)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn create_playlist(
    title: String,
    description: String,
    state: State<'_, AppState>,
) -> Result<PlaylistSummary, String> {
    state
        .engine
        .create_playlist(title, description)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_playlist(
    playlist_uuid: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .engine
        .delete_playlist(playlist_uuid)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn add_track_to_playlist(
    playlist_uuid: String,
    track_id: u64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .engine
        .add_track_to_playlist(playlist_uuid, track_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn remove_track_from_playlist(
    playlist_uuid: String,
    index: u32,
    track_id: u64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .engine
        .remove_track_from_playlist(playlist_uuid, index as usize, track_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn update_playlist(
    playlist_uuid: String,
    title: String,
    description: String,
    state: State<'_, AppState>,
) -> Result<PlaylistSummary, String> {
    state
        .engine
        .update_playlist(playlist_uuid, title, description)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn move_playlist_track(
    playlist_uuid: String,
    from_index: u32,
    to_index: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .engine
        .move_playlist_track(playlist_uuid, from_index as usize, to_index as usize)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn duplicate_playlist(
    playlist_uuid: String,
    new_title: Option<String>,
    state: State<'_, AppState>,
) -> Result<PlaylistSummary, String> {
    state
        .engine
        .duplicate_playlist(playlist_uuid, new_title)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn export_playlist(
    playlist_uuid: String,
    format: PlaylistExportFormat,
    state: State<'_, AppState>,
) -> Result<PlaylistExportPayload, String> {
    state
        .engine
        .export_playlist(playlist_uuid, format)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn import_playlist(
    title: String,
    content: String,
    format: PlaylistExportFormat,
    state: State<'_, AppState>,
) -> Result<PlaylistImportResult, String> {
    state
        .engine
        .import_playlist(title, content, format)
        .await
        .map_err(|e| e.to_string())
}
