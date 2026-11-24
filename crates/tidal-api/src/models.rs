//! Désérialisation tolérante des réponses JSON de l'API TIDAL v1 vers les modèles
//! compacts `tidal_core`.

use serde::Deserialize;
use serde_json::Value;
use tidal_core::{AlbumSummary, ArtistSummary, PlaylistSummary, SearchResults, TrackSummary};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PagedResponse<T> {
    #[serde(default)]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub total_number_of_items: usize,
    #[serde(default = "Vec::new")]
    pub items: Vec<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawArtist {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub picture: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawAlbum {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub cover: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub audio_quality: Option<String>,
    #[serde(default)]
    pub number_of_tracks: u32,
    #[serde(default)]
    pub artist: Option<RawArtist>,
    #[serde(default)]
    pub artists: Vec<RawArtist>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawMediaMetadata {
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawTrack {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub duration: u32,
    #[serde(default)]
    pub explicit: bool,
    #[serde(default)]
    pub audio_quality: Option<String>,
    #[serde(default)]
    pub media_metadata: Option<RawMediaMetadata>,
    #[serde(default)]
    pub artist: Option<RawArtist>,
    #[serde(default)]
    pub artists: Vec<RawArtist>,
    #[serde(default)]
    pub album: Option<RawAlbum>,
}

/// Wrapper utilisé par `/favorites/tracks`, `/playlists/{uuid}/items` et `/albums/{id}/items`.
#[derive(Debug, Deserialize)]
pub struct TrackItemWrapper {
    pub item: Option<RawTrack>,
    #[serde(flatten)]
    pub direct: Option<RawTrack>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPlaylistCreator {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPlaylist {
    #[serde(default)]
    pub uuid: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub number_of_tracks: u32,
    #[serde(default)]
    pub square_image: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub creator: Option<RawPlaylistCreator>,
}

#[derive(Debug, Deserialize)]
pub struct PlaylistItemWrapper {
    pub playlist: Option<RawPlaylist>,
    #[serde(flatten)]
    pub direct: Option<RawPlaylist>,
}

#[derive(Debug, Deserialize)]
pub struct RawSearchResponse {
    #[serde(default)]
    pub tracks: Option<PagedResponse<RawTrack>>,
    #[serde(default)]
    pub albums: Option<PagedResponse<RawAlbum>>,
    #[serde(default)]
    pub playlists: Option<PagedResponse<RawPlaylist>>,
    #[serde(default)]
    pub artists: Option<PagedResponse<RawArtist>>,
}

impl RawTrack {
    pub fn into_summary(self) -> Option<TrackSummary> {
        if self.id == 0 || self.title.is_empty() {
            return None;
        }
        let title = match self.version.as_deref().map(str::trim) {
            Some(ver) if !ver.is_empty() && !self.title.contains(ver) => {
                format!("{} ({ver})", self.title)
            }
            _ => self.title,
        };
        let artist_id = self
            .artists
            .iter()
            .chain(self.artist.iter())
            .find_map(|a| (a.id > 0).then_some(a.id));
        let artist_name = if !self.artists.is_empty() {
            self.artists
                .iter()
                .map(|a| a.name.as_str())
                .filter(|n| !n.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            self.artist
                .map(|a| a.name)
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "Artiste inconnu".into())
        };
        let (album_id, album_title, cover_id) = match self.album {
            Some(alb) => (
                (alb.id > 0).then_some(alb.id),
                alb.title,
                alb.cover,
            ),
            None => (None, String::new(), None),
        };
        let is_hires_tag = self.media_metadata.as_ref().is_some_and(|m| {
            m.tags
                .iter()
                .any(|t| t.eq_ignore_ascii_case("HIRES_LOSSLESS") || t.eq_ignore_ascii_case("HI_RES_LOSSLESS"))
        });
        let audio_quality = if is_hires_tag {
            Some("HI_RES_LOSSLESS".into())
        } else {
            self.audio_quality
        };

        Some(TrackSummary {
            id: self.id,
            title,
            artist_name,
            artist_id,
            album_id,
            album_title,
            cover_id,
            duration_secs: self.duration,
            audio_quality,
            explicit: self.explicit,
        })
    }
}

impl TrackItemWrapper {
    pub fn into_track(self) -> Option<TrackSummary> {
        self.item
            .and_then(RawTrack::into_summary)
            .or_else(|| self.direct.and_then(RawTrack::into_summary))
    }
}

impl RawPlaylist {
    pub fn into_summary(self) -> Option<PlaylistSummary> {
        if self.uuid.is_empty() || self.title.is_empty() {
            return None;
        }
        let creator = self
            .creator
            .and_then(|c| c.name)
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "TIDAL".into());
        Some(PlaylistSummary {
            uuid: self.uuid,
            title: self.title,
            description: self.description.filter(|d| !d.trim().is_empty()),
            creator,
            number_of_tracks: self.number_of_tracks,
            image_id: self.square_image.or(self.image),
        })
    }
}

impl PlaylistItemWrapper {
    pub fn into_playlist(self) -> Option<PlaylistSummary> {
        self.playlist
            .and_then(RawPlaylist::into_summary)
            .or_else(|| self.direct.and_then(RawPlaylist::into_summary))
    }
}

impl RawAlbum {
    pub fn into_summary(self) -> Option<AlbumSummary> {
        if self.id == 0 || self.title.is_empty() {
            return None;
        }
        let artist_id = self
            .artists
            .iter()
            .chain(self.artist.iter())
            .find_map(|a| (a.id > 0).then_some(a.id));
        let artist_name = if !self.artists.is_empty() {
            self.artists
                .iter()
                .map(|a| a.name.as_str())
                .filter(|n| !n.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            self.artist
                .map(|a| a.name)
                .unwrap_or_else(|| "Artiste inconnu".into())
        };
        Some(AlbumSummary {
            id: self.id,
            title: self.title,
            artist_name,
            artist_id,
            cover_id: self.cover,
            release_date: self.release_date,
            audio_quality: self.audio_quality,
            number_of_tracks: self.number_of_tracks,
        })
    }
}

impl RawArtist {
    pub fn into_summary(self) -> Option<ArtistSummary> {
        if self.id == 0 || self.name.is_empty() {
            return None;
        }
        Some(ArtistSummary {
            id: self.id,
            name: self.name,
            picture_id: self.picture,
        })
    }
}

impl RawSearchResponse {
    pub fn into_results(self) -> SearchResults {
        SearchResults {
            tracks: self
                .tracks
                .map(|p| p.items.into_iter().filter_map(RawTrack::into_summary).collect())
                .unwrap_or_default(),
            albums: self
                .albums
                .map(|p| p.items.into_iter().filter_map(RawAlbum::into_summary).collect())
                .unwrap_or_default(),
            playlists: self
                .playlists
                .map(|p| p.items.into_iter().filter_map(RawPlaylist::into_summary).collect())
                .unwrap_or_default(),
            artists: self
                .artists
                .map(|p| p.items.into_iter().filter_map(RawArtist::into_summary).collect())
                .unwrap_or_default(),
        }
    }
}

/// Extrait récursivement les pistes présentes dans les pages dynamiques TIDAL (`/v1/pages/hires`, etc.).
pub fn extract_tracks_from_page_json(root: &Value, max_tracks: usize) -> Vec<TrackSummary> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    walk_page_value(root, &mut out, &mut seen, max_tracks);
    out
}

/// Extrait pistes, albums et playlists (avec leurs vraies miniatures) depuis une page dynamique TIDAL.
pub fn extract_results_from_page_json(
    root: &Value,
    max_tracks: usize,
    max_albums: usize,
    max_playlists: usize,
) -> SearchResults {
    let mut out = SearchResults::default();
    let mut seen_tracks = std::collections::HashSet::new();
    let mut seen_albums = std::collections::HashSet::new();
    let mut seen_playlists = std::collections::HashSet::new();
    walk_page_rich(
        root,
        &mut out,
        &mut seen_tracks,
        &mut seen_albums,
        &mut seen_playlists,
        max_tracks,
        max_albums,
        max_playlists,
    );
    // Complète les albums à partir des pistes extraites si la page contenait surtout des TRACK_LIST
    if out.albums.len() < max_albums {
        for t in &out.tracks {
            if let Some(aid) = t.album_id {
                if t.cover_id.is_some() && seen_albums.insert(aid) {
                    out.albums.push(AlbumSummary {
                        id: aid,
                        title: t.album_title.clone(),
                        artist_name: t.artist_name.clone(),
                        artist_id: t.artist_id,
                        cover_id: t.cover_id.clone(),
                        release_date: None,
                        audio_quality: t.audio_quality.clone(),
                        number_of_tracks: 0,
                    });
                    if out.albums.len() >= max_albums {
                        break;
                    }
                }
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn walk_page_rich(
    val: &Value,
    out: &mut SearchResults,
    seen_tracks: &mut std::collections::HashSet<u64>,
    seen_albums: &mut std::collections::HashSet<u64>,
    seen_playlists: &mut std::collections::HashSet<String>,
    max_tracks: usize,
    max_albums: usize,
    max_playlists: usize,
) {
    match val {
        Value::Object(map) => {
            // 1. Piste TIDAL
            if out.tracks.len() < max_tracks
                && map.contains_key("duration")
                && map.contains_key("album")
                && map.contains_key("title")
                && map.get("id").and_then(Value::as_u64).is_some()
            {
                if let Ok(raw) = serde_json::from_value::<RawTrack>(val.clone()) {
                    if let Some(summary) = raw.into_summary() {
                        if seen_tracks.insert(summary.id) {
                            out.tracks.push(summary);
                            return;
                        }
                    }
                }
            }
            // 2. Playlist TIDAL
            if out.playlists.len() < max_playlists
                && map.get("uuid").and_then(Value::as_str).is_some()
                && map.get("title").and_then(Value::as_str).is_some()
                && (map.contains_key("numberOfTracks")
                    || map.contains_key("squareImage")
                    || map.contains_key("image"))
            {
                if let Ok(raw) = serde_json::from_value::<RawPlaylist>(val.clone()) {
                    if let Some(pl) = raw.into_summary() {
                        if seen_playlists.insert(pl.uuid.clone()) {
                            out.playlists.push(pl);
                            return;
                        }
                    }
                }
            }
            // 3. Album TIDAL
            if out.albums.len() < max_albums
                && map.get("id").and_then(Value::as_u64).is_some()
                && map.get("title").and_then(Value::as_str).is_some()
                && map.get("cover").and_then(Value::as_str).is_some()
                && !map.contains_key("album")
            {
                if let Ok(raw) = serde_json::from_value::<RawAlbum>(val.clone()) {
                    if let Some(alb) = raw.into_summary() {
                        if seen_albums.insert(alb.id) {
                            out.albums.push(alb);
                            return;
                        }
                    }
                }
            }
            for child in map.values() {
                walk_page_rich(
                    child,
                    out,
                    seen_tracks,
                    seen_albums,
                    seen_playlists,
                    max_tracks,
                    max_albums,
                    max_playlists,
                );
            }
        }
        Value::Array(items) => {
            for item in items {
                walk_page_rich(
                    item,
                    out,
                    seen_tracks,
                    seen_albums,
                    seen_playlists,
                    max_tracks,
                    max_albums,
                    max_playlists,
                );
            }
        }
        _ => {}
    }
}

/// Parse un document JSON:API officiel TIDAL OpenAPI v2 (`https://openapi.tidal.com/v2`)
/// composé de `data` + `included` (`tracks`, `albums`, `artists`, `playlists`, `artworks`).
pub fn parse_jsonapi_results(doc: &Value) -> SearchResults {
    let mut results = SearchResults::default();
    let mut resources: Vec<&Value> = Vec::new();
    if let Some(data) = doc.get("data") {
        match data {
            Value::Array(arr) => resources.extend(arr.iter()),
            Value::Object(_) => resources.push(data),
            _ => {}
        }
    }
    if let Some(inc) = doc.get("included").and_then(Value::as_array) {
        resources.extend(inc.iter());
    }

    let mut artworks: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut artist_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut album_meta: std::collections::HashMap<String, (String, Option<String>)> =
        std::collections::HashMap::new();

    // Passe 1 : indexe artworks, artists, albums
    for res in &resources {
        let rtype = res.get("type").and_then(Value::as_str).unwrap_or_default();
        let id = res.get("id").and_then(Value::as_str).unwrap_or_default();
        let attrs = res.get("attributes");
        match rtype {
            "artworks" => {
                if let Some(href) = pick_best_artwork_href(attrs) {
                    artworks.insert(id.to_string(), href);
                }
            }
            "artists" => {
                if let Some(name) = attrs.and_then(|a| a.get("name")).and_then(Value::as_str) {
                    artist_names.insert(id.to_string(), name.to_string());
                }
            }
            _ => {}
        }
    }

    // Passe 2 : construit albums & playlists en résolvant leurs relations artworks/artists
    let mut seen_albums = std::collections::HashSet::new();
    let mut seen_playlists = std::collections::HashSet::new();
    let mut seen_tracks = std::collections::HashSet::new();
    let mut seen_artists = std::collections::HashSet::new();

    for res in &resources {
        let rtype = res.get("type").and_then(Value::as_str).unwrap_or_default();
        let id_str = res.get("id").and_then(Value::as_str).unwrap_or_default();
        let attrs = res.get("attributes");
        let rels = res.get("relationships");

        match rtype {
            "albums" => {
                let title = attrs
                    .and_then(|a| a.get("title"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if title.is_empty() {
                    continue;
                }
                let cover = first_rel_id(rels, "coverArt")
                    .and_then(|aid| artworks.get(&aid).cloned());
                album_meta.insert(id_str.to_string(), (title.clone(), cover.clone()));

                if let Ok(num_id) = id_str.parse::<u64>() {
                    if seen_albums.insert(num_id) {
                        let artist_rel = first_rel_id(rels, "artists");
                        let artist_name = artist_rel
                            .as_deref()
                            .and_then(|aid| artist_names.get(aid).cloned())
                            .unwrap_or_else(|| "TIDAL".into());
                        let artist_id = artist_rel
                            .as_deref()
                            .and_then(|s| s.parse::<u64>().ok());
                        let quality = extract_jsonapi_quality(attrs);
                        let number_of_tracks = attrs
                            .and_then(|a| a.get("numberOfItems"))
                            .and_then(Value::as_u64)
                            .unwrap_or(0) as u32;
                        let release_date = attrs
                            .and_then(|a| a.get("releaseDate"))
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        results.albums.push(AlbumSummary {
                            id: num_id,
                            title,
                            artist_name,
                            artist_id,
                            cover_id: cover,
                            release_date,
                            audio_quality: quality,
                            number_of_tracks,
                        });
                    }
                }
            }
            "playlists" => {
                let title = attrs
                    .and_then(|a| a.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if !id_str.is_empty() && !title.is_empty() && seen_playlists.insert(id_str.to_string()) {
                    let image_id = first_rel_id(rels, "coverArt")
                        .and_then(|aid| artworks.get(&aid).cloned());
                    let number_of_tracks = attrs
                        .and_then(|a| a.get("numberOfItems"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as u32;
                    let description = attrs
                        .and_then(|a| a.get("description"))
                        .and_then(Value::as_str)
                        .filter(|d| !d.trim().is_empty())
                        .map(str::to_string);
                    results.playlists.push(PlaylistSummary {
                        uuid: id_str.to_string(),
                        title,
                        description,
                        creator: "TIDAL".into(),
                        number_of_tracks,
                        image_id,
                    });
                }
            }
            "artists" => {
                if let (Ok(num_id), Some(name)) = (
                    id_str.parse::<u64>(),
                    attrs.and_then(|a| a.get("name")).and_then(Value::as_str),
                ) {
                    if !name.is_empty() && seen_artists.insert(num_id) {
                        let pic = first_rel_id(rels, "profileArt")
                            .and_then(|aid| artworks.get(&aid).cloned());
                        results.artists.push(ArtistSummary {
                            id: num_id,
                            name: name.to_string(),
                            picture_id: pic,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    // Passe 3 : tracks (après albums pour récupérer titre + pochette)
    for res in &resources {
        if res.get("type").and_then(Value::as_str) != Some("tracks") {
            continue;
        }
        let id_str = res.get("id").and_then(Value::as_str).unwrap_or_default();
        let Ok(track_id) = id_str.parse::<u64>() else {
            continue;
        };
        let attrs = res.get("attributes");
        let rels = res.get("relationships");
        let title = attrs
            .and_then(|a| a.get("title"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        // Un identifiant de relation (`{"id":"555","type":"tracks"}`, sans attributs)
        // ne doit pas masquer la ressource complète présente dans `included`.
        if title.is_empty() || !seen_tracks.insert(track_id) {
            continue;
        }
        let duration_secs = attrs
            .and_then(|a| a.get("duration"))
            .and_then(Value::as_str)
            .map(parse_iso8601_duration_secs)
            .unwrap_or(0);
        let explicit = attrs
            .and_then(|a| a.get("explicit"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let audio_quality = extract_jsonapi_quality(attrs);
        let artist_rel = first_rel_id(rels, "artists");
        let artist_name = artist_rel
            .as_deref()
            .and_then(|aid| artist_names.get(aid).cloned())
            .unwrap_or_else(|| "TIDAL".into());
        let artist_id = artist_rel.as_deref().and_then(|s| s.parse::<u64>().ok());
        let alb_rel = first_rel_id(rels, "albums");
        let album_id = alb_rel.as_deref().and_then(|s| s.parse::<u64>().ok());
        let (album_title, cover_id) = alb_rel
            .as_deref()
            .and_then(|aid| album_meta.get(aid).cloned())
            .unwrap_or_else(|| (String::new(), None));

        results.tracks.push(TrackSummary {
            id: track_id,
            title,
            artist_name,
            artist_id,
            album_id,
            album_title,
            cover_id,
            duration_secs,
            audio_quality,
            explicit,
        });
    }

    results
}

fn first_rel_id(rels: Option<&Value>, key: &str) -> Option<String> {
    let data = rels?.get(key)?.get("data")?;
    match data {
        Value::Array(arr) => arr
            .first()
            .and_then(|v| v.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string),
        Value::Object(obj) => obj.get("id").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

fn pick_best_artwork_href(attrs: Option<&Value>) -> Option<String> {
    let files = attrs?.get("files")?.as_array()?;
    let mut best: Option<(u64, String)> = None;
    for f in files {
        let Some(href) = f.get("href").and_then(Value::as_str) else {
            continue;
        };
        let w = f
            .pointer("/meta/width")
            .and_then(Value::as_u64)
            .unwrap_or(320);
        // Privilégie une miniature nette proche de 320-640 px
        let score = (w as i64 - 480).unsigned_abs();
        match &best {
            None => best = Some((score, href.to_string())),
            Some((best_score, _)) if score < *best_score => best = Some((score, href.to_string())),
            _ => {}
        }
    }
    best.map(|(_, url)| url)
}

fn extract_jsonapi_quality(attrs: Option<&Value>) -> Option<String> {
    let tags = attrs?.get("mediaTags")?.as_array()?;
    if tags
        .iter()
        .any(|t| t.as_str() == Some("HIRES_LOSSLESS") || t.as_str() == Some("HI_RES_LOSSLESS"))
    {
        return Some("HI_RES_LOSSLESS".into());
    }
    if tags.iter().any(|t| t.as_str() == Some("LOSSLESS")) {
        return Some("LOSSLESS".into());
    }
    None
}

/// Parse une durée ISO 8601 (`PT3M5S`, `PT1H2M10S`) en secondes.
pub fn parse_iso8601_duration_secs(iso: &str) -> u32 {
    let s = iso.trim().strip_prefix("PT").unwrap_or(iso);
    let mut total = 0u32;
    let mut num = 0u32;
    for ch in s.chars() {
        if let Some(d) = ch.to_digit(10) {
            num = num.saturating_mul(10).saturating_add(d);
        } else {
            match ch {
                'H' => total = total.saturating_add(num.saturating_mul(3600)),
                'M' => total = total.saturating_add(num.saturating_mul(60)),
                'S' => total = total.saturating_add(num),
                _ => {}
            }
            num = 0;
        }
    }
    total
}

fn walk_page_value(
    val: &Value,
    out: &mut Vec<TrackSummary>,
    seen: &mut std::collections::HashSet<u64>,
    max_tracks: usize,
) {
    if out.len() >= max_tracks {
        return;
    }
    match val {
        Value::Object(map) => {
            // Un objet piste TIDAL comporte au minimum `id`, `title`, `duration` et `album`.
            if map.contains_key("duration")
                && map.contains_key("album")
                && map.contains_key("title")
                && map.get("id").and_then(Value::as_u64).is_some()
            {
                if let Ok(raw) = serde_json::from_value::<RawTrack>(val.clone()) {
                    if let Some(summary) = raw.into_summary() {
                        if seen.insert(summary.id) {
                            out.push(summary);
                            return;
                        }
                    }
                }
            }
            for child in map.values() {
                walk_page_value(child, out, seen, max_tracks);
                if out.len() >= max_tracks {
                    return;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                walk_page_value(item, out, seen, max_tracks);
                if out.len() >= max_tracks {
                    return;
                }
            }
        }
        _ => {}
    }
}
