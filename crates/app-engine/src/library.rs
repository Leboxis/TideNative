use std::collections::HashSet;
use tidal_core::{
    AlbumSummary, ArtistSummary, ExploreHubData, MixSummary, PlaylistFolder, PlaylistSummary,
    TrackSortOrder, TrackSummary,
};

pub const RECENT_SEARCHES_LIMIT: usize = 10;

#[derive(Debug, Default, Clone)]
pub struct LibraryState {
    pub favorites: Vec<TrackSummary>,
    pub favorite_ids: HashSet<u64>,
    pub playlists: Vec<PlaylistSummary>,
    pub playlist_folders: Vec<PlaylistFolder>,
    pub favorite_albums: Vec<AlbumSummary>,
    pub favorite_album_ids: HashSet<u64>,
    pub favorite_artists: Vec<ArtistSummary>,
    pub favorite_artist_ids: HashSet<u64>,
    pub user_mixes: Vec<MixSummary>,
    pub discover_tracks: Vec<TrackSummary>,
    pub explore_hub: Option<ExploreHubData>,
    pub recent_searches: Vec<String>,
    pub search_seq: u64,
}

impl LibraryState {
    pub fn next_search_seq(&mut self) -> u64 {
        self.search_seq = self.search_seq.wrapping_add(1);
        self.search_seq
    }

    pub fn is_current_search(&self, seq: u64) -> bool {
        self.search_seq == seq
    }

    pub fn set_favorites(&mut self, tracks: Vec<TrackSummary>) {
        self.favorite_ids = tracks.iter().map(|t| t.id).collect();
        self.favorites = tracks;
    }

    pub fn set_favorite_albums(&mut self, albums: Vec<AlbumSummary>) {
        self.favorite_album_ids = albums.iter().map(|a| a.id).collect();
        self.favorite_albums = albums;
    }

    pub fn set_favorite_artists(&mut self, artists: Vec<ArtistSummary>) {
        self.favorite_artist_ids = artists.iter().map(|a| a.id).collect();
        self.favorite_artists = artists;
    }

    pub fn toggle_favorite_track(&mut self, track: &TrackSummary) -> bool {
        if self.favorite_ids.remove(&track.id) {
            self.favorites.retain(|t| t.id != track.id);
            false
        } else {
            self.favorite_ids.insert(track.id);
            if !self.favorites.iter().any(|t| t.id == track.id) {
                self.favorites.insert(0, track.clone());
            }
            true
        }
    }

    pub fn toggle_favorite_album(
        &mut self,
        album_id: u64,
        album: Option<&AlbumSummary>,
    ) -> bool {
        if self.favorite_album_ids.remove(&album_id) {
            self.favorite_albums.retain(|a| a.id != album_id);
            false
        } else {
            self.favorite_album_ids.insert(album_id);
            if let Some(a) = album {
                if !self.favorite_albums.iter().any(|entry| entry.id == album_id) {
                    self.favorite_albums.insert(0, a.clone());
                }
            }
            true
        }
    }

    pub fn toggle_favorite_artist(
        &mut self,
        artist_id: u64,
        artist: Option<&ArtistSummary>,
    ) -> bool {
        if self.favorite_artist_ids.remove(&artist_id) {
            self.favorite_artists.retain(|a| a.id != artist_id);
            false
        } else {
            self.favorite_artist_ids.insert(artist_id);
            if let Some(a) = artist {
                if !self.favorite_artists.iter().any(|entry| entry.id == artist_id) {
                    self.favorite_artists.insert(0, a.clone());
                }
            }
            true
        }
    }

    pub fn record_recent_search(&mut self, query: &str) {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return;
        }
        self.recent_searches
            .retain(|q| !q.eq_ignore_ascii_case(trimmed));
        self.recent_searches.insert(0, trimmed.to_string());
        self.recent_searches.truncate(RECENT_SEARCHES_LIMIT);
    }
}

pub fn is_likely_ai_generated(track: &TrackSummary) -> bool {
    let haystack = format!(
        "{} {} {}",
        track.title, track.artist_name, track.album_title
    )
    .to_ascii_lowercase();
    const MARKERS: &[&str] = &[
        "ai generated",
        "ai-generated",
        "(ai cover)",
        "[ai cover]",
        "ai cover",
        "ai voice",
        "suno ai",
        "udio ai",
        "made with ai",
        "généré par ia",
    ];
    MARKERS.iter().any(|m| haystack.contains(m))
}

pub fn filter_tracks_with_positions(
    tracks: Vec<TrackSummary>,
    allow_explicit: bool,
    allow_ai: bool,
) -> (Vec<TrackSummary>, Vec<usize>) {
    if allow_explicit && allow_ai {
        let positions = (0..tracks.len()).collect();
        return (tracks, positions);
    }
    let mut kept = Vec::with_capacity(tracks.len());
    let mut positions = Vec::with_capacity(tracks.len());
    for (idx, track) in tracks.into_iter().enumerate() {
        if !allow_explicit && track.explicit {
            continue;
        }
        if !allow_ai && is_likely_ai_generated(&track) {
            continue;
        }
        kept.push(track);
        positions.push(idx);
    }
    (kept, positions)
}

pub fn filter_tracks(
    tracks: Vec<TrackSummary>,
    allow_explicit: bool,
    allow_ai: bool,
) -> Vec<TrackSummary> {
    filter_tracks_with_positions(tracks, allow_explicit, allow_ai).0
}

pub fn filter_and_sort_tracks(
    tracks: &[TrackSummary],
    allow_explicit: bool,
    allow_ai: bool,
    query: &str,
    sort: TrackSortOrder,
) -> Vec<TrackSummary> {
    let needle = query.trim().to_ascii_lowercase();
    let mut result: Vec<TrackSummary> = tracks
        .iter()
        .filter(|track| {
            if !allow_explicit && track.explicit {
                return false;
            }
            if !allow_ai && is_likely_ai_generated(track) {
                return false;
            }
            if !needle.is_empty() {
                let matches = track.title.to_ascii_lowercase().contains(&needle)
                    || track.artist_name.to_ascii_lowercase().contains(&needle)
                    || track.album_title.to_ascii_lowercase().contains(&needle);
                if !matches {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

    match sort {
        TrackSortOrder::Default => {}
        TrackSortOrder::TitleAsc => {
            result.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        }
        TrackSortOrder::ArtistAsc => {
            result.sort_by(|a, b| {
                a.artist_name
                    .to_lowercase()
                    .cmp(&b.artist_name.to_lowercase())
                    .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            });
        }
        TrackSortOrder::AlbumAsc => {
            result.sort_by(|a, b| {
                a.album_title
                    .to_lowercase()
                    .cmp(&b.album_title.to_lowercase())
                    .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            });
        }
        TrackSortOrder::DurationDesc => {
            result.sort_by(|a, b| b.duration_secs.cmp(&a.duration_secs));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tracks() -> Vec<TrackSummary> {
        vec![
            TrackSummary {
                id: 1,
                title: "Zebra".into(),
                artist_name: "Artist A".into(),
                artist_id: Some(1),
                album_id: Some(1),
                album_title: "Album 1".into(),
                cover_id: None,
                duration_secs: 200,
                audio_quality: Some("HI_RES_LOSSLESS".into()),
                explicit: true,
            },
            TrackSummary {
                id: 2,
                title: "Alpha".into(),
                artist_name: "Artist B".into(),
                artist_id: Some(2),
                album_id: Some(2),
                album_title: "Album 2".into(),
                cover_id: None,
                duration_secs: 150,
                audio_quality: Some("LOSSLESS".into()),
                explicit: false,
            },
            TrackSummary {
                id: 3,
                title: "Neon Beat (AI Cover)".into(),
                artist_name: "Suno AI".into(),
                artist_id: Some(3),
                album_id: Some(3),
                album_title: "Synth".into(),
                cover_id: None,
                duration_secs: 140,
                audio_quality: Some("LOSSLESS".into()),
                explicit: false,
            },
        ]
    }

    #[test]
    fn stale_search_seq_is_rejected_and_explicit_ai_filters_apply() {
        let mut lib = LibraryState::default();
        let seq1 = lib.next_search_seq();
        let seq2 = lib.next_search_seq();
        assert!(!lib.is_current_search(seq1));
        assert!(lib.is_current_search(seq2));

        let filtered =
            filter_and_sort_tracks(&sample_tracks(), false, false, "", TrackSortOrder::TitleAsc);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, 2);
        assert!(filtered.iter().all(|t| !t.explicit));
    }

    #[test]
    fn recent_searches_deduplicate_case_insensitively() {
        let mut lib = LibraryState::default();
        lib.record_recent_search("Daft Punk");
        lib.record_recent_search("Radiohead");
        lib.record_recent_search("daft punk");
        assert_eq!(lib.recent_searches, vec!["daft punk", "Radiohead"]);
    }
}
