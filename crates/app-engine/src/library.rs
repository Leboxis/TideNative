use std::collections::{HashMap, HashSet, VecDeque};
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
    pub discover_refreshed_at: Option<u64>,
    pub discover_rotation: u64,
    pub discover_renewal_pending: bool,
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

fn normalized_artist_key(artist: &str) -> String {
    artist
        .split([',', '&', ';'])
        .next()
        .unwrap_or(artist)
        .trim()
        .to_ascii_lowercase()
}

fn normalized_title_key(title: &str) -> String {
    let lower = title.to_ascii_lowercase();
    let base = lower
        .split(['(', '['])
        .next()
        .unwrap_or(&lower)
        .split(" - ")
        .next()
        .unwrap_or(&lower);
    base.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Sélectionne jusqu'à `limit` pistes graines issues d'artistes différents
/// (en combinant écoutes récentes et favoris avec une rotation pour varier d'une session à l'autre).
pub fn pick_diverse_seed_tracks(
    recent: &[TrackSummary],
    favorites: &[TrackSummary],
    rotation: u64,
    limit: usize,
) -> Vec<u64> {
    if limit == 0 {
        return Vec::new();
    }
    let mut pool: Vec<&TrackSummary> = Vec::new();
    let mut seen_ids = HashSet::new();
    for track in recent.iter().take(15).chain(favorites.iter().take(120)) {
        if seen_ids.insert(track.id) {
            pool.push(track);
        }
    }
    if pool.is_empty() {
        return Vec::new();
    }

    let len = pool.len();
    let start = (rotation as usize * 7) % len;
    let stride = if len > 5 { 3 } else { 1 };
    let mut chosen = Vec::with_capacity(limit);
    let mut seen_artists = HashSet::new();
    let mut visited = HashSet::new();

    let mut idx = start;
    for _ in 0..len {
        if chosen.len() >= limit {
            break;
        }
        if visited.insert(idx) {
            let track = pool[idx];
            let artist_key = normalized_artist_key(&track.artist_name);
            if seen_artists.insert(artist_key) {
                chosen.push(track.id);
            }
        }
        idx = (idx + stride) % len;
    }

    if chosen.len() < limit {
        for track in pool {
            if chosen.len() >= limit {
                break;
            }
            if !chosen.contains(&track.id) {
                chosen.push(track.id);
            }
        }
    }
    chosen
}

/// Sélectionne jusqu'à `limit` artistes suivis différents en appliquant une rotation.
pub fn pick_diverse_seed_artists(
    artists: &[ArtistSummary],
    rotation: u64,
    limit: usize,
) -> Vec<u64> {
    if artists.is_empty() || limit == 0 {
        return Vec::new();
    }
    let len = artists.len();
    let start = (rotation as usize * 3) % len;
    let mut chosen = Vec::with_capacity(limit.min(len));
    for step in 0..len {
        if chosen.len() >= limit {
            break;
        }
        let artist = &artists[(start + step * 2) % len];
        if artist.id > 0 && !chosen.contains(&artist.id) {
            chosen.push(artist.id);
        }
    }
    chosen
}

/// Construit une sélection d'accueil variée à partir de plusieurs sources (mixes, radios pistes,
/// radios artistes, catalogue Hi-Res) :
/// - filtre explicite / IA,
/// - déduplique par identifiant et par `(artiste, titre)`,
/// - entrelace les sources en round-robin,
/// - privilégie les découvertes (hors favoris) avant les redécouvertes,
/// - plafonne le nombre de titres par artiste (`max_per_artist`) et évite 2 titres consécutifs du même artiste.
/// `rotation` décale l'ordre des sources et le point de départ dans chacune : la tête de liste
/// (et donc le Hero de l'accueil) change à chaque régénération.
pub fn curate_diverse_discover_tracks(
    buckets: Vec<Vec<TrackSummary>>,
    favorite_ids: &HashSet<u64>,
    allow_explicit: bool,
    allow_ai: bool,
    max_tracks: usize,
    max_per_artist: usize,
    rotation: u64,
) -> Vec<TrackSummary> {
    if max_tracks == 0 || max_per_artist == 0 {
        return Vec::new();
    }

    let mut queues: Vec<VecDeque<TrackSummary>> = buckets
        .into_iter()
        .map(|b| VecDeque::from(filter_tracks(b, allow_explicit, allow_ai)))
        .filter(|q| !q.is_empty())
        .collect();

    if !queues.is_empty() {
        let bucket_offset = (rotation as usize) % queues.len();
        queues.rotate_left(bucket_offset);
        for (i, q) in queues.iter_mut().enumerate() {
            if q.is_empty() {
                continue;
            }
            let start = (rotation as usize)
                .wrapping_mul(7)
                .wrapping_add(i.wrapping_mul(3))
                % q.len();
            q.rotate_left(start);
        }
    }

    let mut seen_ids = HashSet::new();
    let mut seen_signatures = HashSet::new();
    let mut fresh = VecDeque::new();
    let mut familiar = VecDeque::new();

    while queues.iter().any(|q| !q.is_empty()) {
        for q in &mut queues {
            while let Some(track) = q.pop_front() {
                let sig = (
                    normalized_artist_key(&track.artist_name),
                    normalized_title_key(&track.title),
                );
                if !seen_ids.insert(track.id) || !seen_signatures.insert(sig) {
                    continue;
                }
                if favorite_ids.contains(&track.id) {
                    familiar.push_back(track);
                } else {
                    fresh.push_back(track);
                }
                break;
            }
        }
    }

    let mut result: Vec<TrackSummary> = Vec::with_capacity(max_tracks);
    let mut artist_counts: HashMap<String, usize> = HashMap::new();
    let mut last_artist: Option<String> = None;

    let mut push_from_pool = |pool: &mut VecDeque<TrackSummary>, target_len: usize| {
        while result.len() < target_len && !pool.is_empty() {
            let mut picked_idx: Option<usize> = None;
            let mut fallback_idx: Option<usize> = None;

            for (i, candidate) in pool.iter().enumerate() {
                let artist = normalized_artist_key(&candidate.artist_name);
                let count = artist_counts.get(&artist).copied().unwrap_or(0);
                if count >= max_per_artist {
                    continue;
                }
                if last_artist.as_deref() != Some(artist.as_str()) {
                    picked_idx = Some(i);
                    break;
                }
                if fallback_idx.is_none() {
                    fallback_idx = Some(i);
                }
            }

            let Some(idx) = picked_idx.or(fallback_idx) else {
                break;
            };
            let track = pool.remove(idx).expect("valid candidate index");
            let artist = normalized_artist_key(&track.artist_name);
            *artist_counts.entry(artist.clone()).or_insert(0) += 1;
            last_artist = Some(artist);
            result.push(track);
        }
    };

    push_from_pool(&mut fresh, max_tracks);
    push_from_pool(&mut familiar, max_tracks);

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
                dolby_audio: false,
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
                dolby_audio: false,
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
                dolby_audio: false,
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

    fn mk_track(id: u64, title: &str, artist: &str) -> TrackSummary {
        TrackSummary {
            id,
            title: title.into(),
            artist_name: artist.into(),
            artist_id: Some(id),
            album_id: Some(id * 10),
            album_title: "Album".into(),
            cover_id: None,
            duration_secs: 200,
            audio_quality: Some("LOSSLESS".into()),
            explicit: false,
            dolby_audio: false,
        }
    }

    #[test]
    fn curate_diverse_discover_tracks_prioritizes_fresh_and_prevents_artist_clumping() {
        let bucket_a = vec![
            mk_track(1, "Fav Song", "Daft Punk"),
            mk_track(2, "Fresh A1", "Daft Punk"),
            mk_track(3, "Fresh A2", "Daft Punk"),
            mk_track(4, "Fresh A3", "Daft Punk"),
            mk_track(5, "Fresh A1 (Remaster)", "Daft Punk"),
        ];
        let bucket_b = vec![
            mk_track(10, "Pulse", "Justice"),
            mk_track(11, "Genesis", "Justice"),
            mk_track(12, "Midnight", "M83"),
        ];
        let mut fav_ids = HashSet::new();
        fav_ids.insert(1);

        let curated = curate_diverse_discover_tracks(
            vec![bucket_a, bucket_b],
            &fav_ids,
            true,
            false,
            10,
            2,
            0,
        );

        // Fresh tracks come before the already-favorited track (id 1),
        // duplicate remaster (id 5) is removed, and max 2 per artist applies.
        assert!(!curated.iter().any(|t| t.id == 5));
        let daft_count = curated
            .iter()
            .filter(|t| t.artist_name == "Daft Punk")
            .count();
        assert!(daft_count <= 2);

        // No two consecutive tracks share the same primary artist when alternatives exist.
        for pair in curated.windows(2) {
            assert_ne!(
                pair[0].artist_name, pair[1].artist_name,
                "consecutive duplicate artist in curated feed: {:?}",
                curated
            );
        }
    }

    #[test]
    fn pick_diverse_seed_tracks_rotates_and_diversifies_artists() {
        let favs = vec![
            mk_track(1, "T1", "Artist A"),
            mk_track(2, "T2", "Artist A"),
            mk_track(3, "T3", "Artist B"),
            mk_track(4, "T4", "Artist C"),
            mk_track(5, "T5", "Artist D"),
            mk_track(6, "T6", "Artist E"),
        ];
        let s0 = pick_diverse_seed_tracks(&[], &favs, 0, 3);
        let s1 = pick_diverse_seed_tracks(&[], &favs, 1, 3);
        assert_eq!(s0.len(), 3);
        assert_eq!(s1.len(), 3);
        assert_ne!(s0, s1);
    }

    #[test]
    fn curation_rotation_changes_the_head_of_the_selection() {
        // Deux buckets stables (comme les mixes quotidiens) : sans rotation dans la curation,
        // la tête de liste resterait identique à chaque clic sur « Varier la sélection ».
        let mixes = vec![
            mk_track(101, "Mix One A", "Artist M1"),
            mk_track(102, "Mix One B", "Artist M2"),
            mk_track(103, "Mix One C", "Artist M3"),
        ];
        let radio = vec![
            mk_track(201, "Radio A", "Artist R1"),
            mk_track(202, "Radio B", "Artist R2"),
            mk_track(203, "Radio C", "Artist R3"),
        ];
        let empty_favs = HashSet::new();

        let r0 = curate_diverse_discover_tracks(
            vec![mixes.clone(), radio.clone()],
            &empty_favs,
            true,
            false,
            12,
            2,
            0,
        );
        let r1 = curate_diverse_discover_tracks(
            vec![mixes.clone(), radio.clone()],
            &empty_favs,
            true,
            false,
            12,
            2,
            1,
        );
        let r2 = curate_diverse_discover_tracks(
            vec![mixes, radio],
            &empty_favs,
            true,
            false,
            12,
            2,
            2,
        );

        assert!(!r0.is_empty() && !r1.is_empty() && !r2.is_empty());
        assert_ne!(
            r0[0].id, r1[0].id,
            "rotation 1 doit changer le premier titre"
        );
        assert_ne!(
            r1[0].id, r2[0].id,
            "rotation 2 doit changer le premier titre"
        );
    }
}
