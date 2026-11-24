use crate::AppState;
use tauri::State;
use tidal_app_engine::{AlbumDetailData, HybridSearchResults};
use tidal_core::{
    ArtistDetailData, ExploreHubData, SearchResults, SearchSuggestions, TrackCredits, TrackLyrics,
    TrackSummary,
};

#[tauri::command]
#[specta::specta]
pub async fn search_hybrid(
    query: String,
    state: State<'_, AppState>,
) -> Result<HybridSearchResults, String> {
    state
        .engine
        .search_hybrid(query)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn search_suggestions(
    query: String,
    state: State<'_, AppState>,
) -> Result<SearchSuggestions, String> {
    state
        .engine
        .search_suggestions(query)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_explore_hub(state: State<'_, AppState>) -> Result<ExploreHubData, String> {
    state
        .engine
        .get_explore_hub()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_explore_category(
    category: String,
    state: State<'_, AppState>,
) -> Result<SearchResults, String> {
    state
        .engine
        .get_explore_category(category)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_album(album_id: u64, state: State<'_, AppState>) -> Result<AlbumDetailData, String> {
    state
        .engine
        .get_album(album_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_artist(
    artist_id: u64,
    state: State<'_, AppState>,
) -> Result<ArtistDetailData, String> {
    state
        .engine
        .get_artist(artist_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_mix_tracks(
    mix_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<TrackSummary>, String> {
    state
        .engine
        .get_mix_tracks(mix_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_lyrics(
    track_id: u64,
    state: State<'_, AppState>,
) -> Result<Option<TrackLyrics>, String> {
    state
        .engine
        .get_lyrics(track_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_credits(track_id: u64, state: State<'_, AppState>) -> Result<TrackCredits, String> {
    state
        .engine
        .get_credits(track_id)
        .await
        .map_err(|e| e.to_string())
}
