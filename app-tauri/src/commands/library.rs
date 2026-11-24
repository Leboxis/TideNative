use crate::AppState;
use tauri::State;
use tidal_app_engine::{LibrarySyncPayload, PlaylistDetailData};
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
