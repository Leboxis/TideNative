//! Client catalogue TIDAL v1 (`tidal-api`) : session, favoris, playlists,
//! albums, recherche, paroles / crédits / radio et pages Hi-Res, avec pagination
//! parallèle contrôlée (`drain_pages`).

pub mod lastfm;
pub mod models;

use models::{
    PagedResponse, PlaylistItemWrapper, RawAlbum, RawArtist, RawSearchResponse, RawTrack,
    TrackItemWrapper, extract_results_from_page_json, extract_tracks_from_page_json,
    parse_jsonapi_results,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::future::Future;
use std::sync::Arc;
use tidal_auth::client::{Session, TidalClient};
use tidal_auth::AuthError;
use tidal_core::{
    AlbumSummary, ArtistDetailData, ArtistSummary, ExploreHubData, MixSummary, PlaylistFolder,
    PlaylistSummary, SearchResults, SearchSuggestions, TrackCreditGroup, TrackCredits, TrackLyrics,
    TrackSummary, parse_synced_lyrics,
};
use tokio::sync::{RwLock, Semaphore};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("erreur d'authentification ou réseau TIDAL : {0}")]
    Auth(#[from] tidal_auth::AuthError),

    #[error("session utilisateur non initialisée")]
    MissingSession,

    #[error("échec pagination parallèle : {0}")]
    Pagination(String),

    #[error("réponse TIDAL inattendue : {0}")]
    Unexpected(String),
}

pub type Result<T> = std::result::Result<T, ApiError>;

/// Client haut niveau du catalogue TIDAL adossé à [`TidalClient`].
#[derive(Clone)]
pub struct TidalApiClient {
    auth: Arc<TidalClient>,
    session: Arc<RwLock<Option<Session>>>,
}

impl TidalApiClient {
    pub fn new(auth: Arc<TidalClient>) -> Self {
        Self {
            auth,
            session: Arc::new(RwLock::new(None)),
        }
    }

    pub fn auth_client(&self) -> Arc<TidalClient> {
        Arc::clone(&self.auth)
    }

    /// Récupère ou met en cache la session TIDAL courante (`user_id` + `country_code`).
    pub async fn ensure_session(&self) -> Result<Session> {
        if let Some(cached) = self.session.read().await.clone() {
            return Ok(cached);
        }
        let fresh = self.auth.session().await?;
        *self.session.write().await = Some(fresh.clone());
        Ok(fresh)
    }

    /// Charge tous les titres favoris de l'utilisateur via pagination parallèle contrôlée.
    pub async fn favorite_tracks(&self, max_items: usize) -> Result<Vec<TrackSummary>> {
        let sess = self.ensure_session().await?;
        let path = format!("users/{}/favorites/tracks", sess.user_id);
        let country = sess.country_code.clone();
        let auth = Arc::clone(&self.auth);

        let raw: Vec<TrackItemWrapper> = drain_pages(100, max_items, 4, move |offset, limit| {
            let auth = Arc::clone(&auth);
            let path = path.clone();
            let country = country.clone();
            async move {
                let offset_s = offset.to_string();
                let limit_s = limit.to_string();
                auth.get_v1::<PagedResponse<TrackItemWrapper>>(
                    &path,
                    &[
                        ("limit", &limit_s),
                        ("offset", &offset_s),
                        ("order", "DATE"),
                        ("orderDirection", "DESC"),
                        ("countryCode", &country),
                    ],
                )
                .await
                .map_err(ApiError::from)
            }
        })
        .await?;

        Ok(raw.into_iter().filter_map(TrackItemWrapper::into_track).collect())
    }

    /// Charge les playlists créées et favorites de l'utilisateur.
    pub async fn user_playlists(&self, max_items: usize) -> Result<Vec<PlaylistSummary>> {
        let sess = self.ensure_session().await?;
        let path = format!("users/{}/playlistsAndFavoritePlaylists", sess.user_id);
        let country = sess.country_code.clone();
        let auth = Arc::clone(&self.auth);

        let raw: Vec<PlaylistItemWrapper> = drain_pages(50, max_items, 4, move |offset, limit| {
            let auth = Arc::clone(&auth);
            let path = path.clone();
            let country = country.clone();
            async move {
                let offset_s = offset.to_string();
                let limit_s = limit.to_string();
                auth.get_v1::<PagedResponse<PlaylistItemWrapper>>(
                    &path,
                    &[
                        ("limit", &limit_s),
                        ("offset", &offset_s),
                        ("countryCode", &country),
                    ],
                )
                .await
                .map_err(ApiError::from)
            }
        })
        .await?;

        Ok(raw
            .into_iter()
            .filter_map(PlaylistItemWrapper::into_playlist)
            .collect())
    }

    /// Charge les dossiers de playlists TIDAL (`v2/my-collection/playlists/folders`)
    /// ainsi que les playlists contenues dans chaque dossier.
    pub async fn user_playlist_folders(&self) -> Result<Vec<PlaylistFolder>> {
        let sess = self.ensure_session().await?;
        let country = sess.country_code.clone();

        let root_val: Value = match self
            .auth
            .get_v2(
                "my-collection/playlists/folders",
                &[
                    ("folderId", "root"),
                    ("offset", "0"),
                    ("limit", "50"),
                    ("order", "DATE"),
                    ("orderDirection", "DESC"),
                    ("countryCode", &country),
                    ("locale", "fr_FR"),
                    ("deviceType", "BROWSER"),
                ],
            )
            .await
        {
            Ok(v) => v,
            Err(_) => return Ok(Vec::new()),
        };

        let mut folders = Vec::new();
        if let Some(items) = root_val.get("items").and_then(Value::as_array) {
            for item in items {
                let kind = item
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !kind.eq_ignore_ascii_case("FOLDER") {
                    continue;
                }
                let Some(data) = item.get("data") else {
                    continue;
                };
                let id = data
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                let name = data
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if id.is_empty() || name.is_empty() {
                    continue;
                }
                let total = data
                    .get("totalNumberOfItems")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as u32;

                let playlists = self
                    .folder_playlists(&id, &country)
                    .await
                    .unwrap_or_default();
                folders.push(PlaylistFolder {
                    id,
                    name,
                    total_number_of_items: total.max(playlists.len() as u32),
                    playlists,
                });
            }
        }

        Ok(folders)
    }

    async fn folder_playlists(
        &self,
        folder_id: &str,
        country: &str,
    ) -> Result<Vec<PlaylistSummary>> {
        let val: Value = self
            .auth
            .get_v2(
                "my-collection/playlists/folders",
                &[
                    ("folderId", folder_id),
                    ("includeOnly", "PLAYLIST"),
                    ("offset", "0"),
                    ("limit", "50"),
                    ("order", "DATE"),
                    ("orderDirection", "DESC"),
                    ("countryCode", country),
                    ("locale", "fr_FR"),
                    ("deviceType", "BROWSER"),
                ],
            )
            .await?;

        let mut out = Vec::new();
        if let Some(items) = val.get("items").and_then(Value::as_array) {
            for item in items {
                let Some(data) = item.get("data") else {
                    continue;
                };
                if let Ok(raw) = serde_json::from_value::<models::RawPlaylist>(data.clone()) {
                    if let Some(pl) = raw.into_summary() {
                        out.push(pl);
                    }
                }
            }
        }
        Ok(out)
    }

    /// Charge l'intégralité des pistes d'une playlist (supporte 2 000+ pistes).
    pub async fn playlist_tracks(
        &self,
        playlist_uuid: &str,
        max_items: usize,
    ) -> Result<Vec<TrackSummary>> {
        let sess = self.ensure_session().await?;
        let path = format!("playlists/{playlist_uuid}/items");
        let country = sess.country_code.clone();
        let auth = Arc::clone(&self.auth);

        let raw: Vec<TrackItemWrapper> = drain_pages(100, max_items, 4, move |offset, limit| {
            let auth = Arc::clone(&auth);
            let path = path.clone();
            let country = country.clone();
            async move {
                let offset_s = offset.to_string();
                let limit_s = limit.to_string();
                auth.get_v1::<PagedResponse<TrackItemWrapper>>(
                    &path,
                    &[
                        ("limit", &limit_s),
                        ("offset", &offset_s),
                        ("countryCode", &country),
                    ],
                )
                .await
                .map_err(ApiError::from)
            }
        })
        .await?;

        Ok(raw.into_iter().filter_map(TrackItemWrapper::into_track).collect())
    }

    /// Charge les métadonnées complètes d'un album TIDAL (`albums/{id}`).
    pub async fn album_details(&self, album_id: u64) -> Result<Option<AlbumSummary>> {
        let sess = self.ensure_session().await?;
        let path = format!("albums/{album_id}");
        let raw: RawAlbum = self
            .auth
            .get_v1(&path, &[("countryCode", &sess.country_code)])
            .await?;
        Ok(raw.into_summary())
    }

    /// Charge les pistes d'un album TIDAL.
    pub async fn album_tracks(&self, album_id: u64) -> Result<Vec<TrackSummary>> {
        let sess = self.ensure_session().await?;
        let path = format!("albums/{album_id}/items");
        let page: PagedResponse<TrackItemWrapper> = self
            .auth
            .get_v1(
                &path,
                &[
                    ("limit", "100"),
                    ("offset", "0"),
                    ("countryCode", &sess.country_code),
                ],
            )
            .await?;

        Ok(page
            .items
            .into_iter()
            .filter_map(TrackItemWrapper::into_track)
            .collect())
    }

    /// Charge la fiche complète d'un artiste TIDAL : profil, biographie nettoyée, top titres,
    /// albums / EP & singles / compilations et artistes similaires, en une passe parallèle.
    /// Les sections optionnelles (bio, compilations, similaires) tolèrent les 404.
    pub async fn artist_detail(&self, artist_id: u64) -> Result<ArtistDetailData> {
        let sess = self.ensure_session().await?;
        let cc = sess.country_code.as_str();
        let profile_path = format!("artists/{artist_id}");
        let bio_path = format!("artists/{artist_id}/bio");
        let top_path = format!("artists/{artist_id}/toptracks");
        let albums_path = format!("artists/{artist_id}/albums");
        let similar_path = format!("artists/{artist_id}/similar");

        let profile_query = [("countryCode", cc)];
        let bio_query = [("countryCode", cc)];
        let top_query = [("limit", "15"), ("offset", "0"), ("countryCode", cc)];
        let albums_query = [
            ("limit", "30"),
            ("offset", "0"),
            ("filter", "ALBUMS"),
            ("countryCode", cc),
        ];
        let eps_query = [
            ("limit", "30"),
            ("offset", "0"),
            ("filter", "EPSANDSINGLES"),
            ("countryCode", cc),
        ];
        let compilations_query = [
            ("limit", "20"),
            ("offset", "0"),
            ("filter", "COMPILATIONS"),
            ("countryCode", cc),
        ];
        let similar_query = [("limit", "12"), ("offset", "0"), ("countryCode", cc)];

        let (profile, bio, top_tracks, albums, eps_singles, compilations, similar) = tokio::join!(
            self.auth.get_v1::<RawArtist>(&profile_path, &profile_query),
            self.auth.get_v1::<Value>(&bio_path, &bio_query),
            self.auth
                .get_v1::<PagedResponse<TrackItemWrapper>>(&top_path, &top_query),
            self.auth
                .get_v1::<PagedResponse<Value>>(&albums_path, &albums_query),
            self.auth
                .get_v1::<PagedResponse<Value>>(&albums_path, &eps_query),
            self.auth
                .get_v1::<PagedResponse<Value>>(&albums_path, &compilations_query),
            self.auth
                .get_v1::<PagedResponse<Value>>(&similar_path, &similar_query),
        );

        let artist = profile?
            .into_summary()
            .ok_or_else(|| ApiError::Unexpected(format!("artiste TIDAL {artist_id} introuvable")))?;
        let bio = bio.ok().and_then(|value| {
            value
                .get("text")
                .or_else(|| value.get("summary"))
                .and_then(Value::as_str)
                .map(clean_tidal_bio)
                .filter(|text| !text.is_empty())
        });

        Ok(ArtistDetailData {
            artist,
            bio,
            top_tracks: top_tracks
                .map(|page| {
                    page.items
                        .into_iter()
                        .filter_map(TrackItemWrapper::into_track)
                        .collect()
                })
                .unwrap_or_default(),
            albums: albums
                .map(|page| parse_album_items(&page.items))
                .unwrap_or_default(),
            eps_singles: eps_singles
                .map(|page| parse_album_items(&page.items))
                .unwrap_or_default(),
            compilations: compilations
                .map(|page| parse_album_items(&page.items))
                .unwrap_or_default(),
            similar_artists: similar
                .map(|page| parse_artist_items(&page.items))
                .unwrap_or_default(),
        })
    }

    /// Charge la radio d'un artiste (v1 `artists/{id}/radio`, repli OpenAPI v2 `relationships/radio`).
    pub async fn artist_radio(&self, artist_id: u64) -> Result<Vec<TrackSummary>> {
        let sess = self.ensure_session().await?;
        let path = format!("artists/{artist_id}/radio");
        if let Ok(page) = self
            .auth
            .get_v1::<PagedResponse<TrackItemWrapper>>(
                &path,
                &[("limit", "50"), ("offset", "0"), ("countryCode", &sess.country_code)],
            )
            .await
        {
            let tracks: Vec<TrackSummary> = page
                .items
                .into_iter()
                .filter_map(TrackItemWrapper::into_track)
                .collect();
            if !tracks.is_empty() {
                return Ok(tracks);
            }
        }
        let oa_path = format!("artists/{artist_id}/relationships/radio");
        let doc: Value = self
            .auth
            .get_openapi_v2(
                &oa_path,
                &[
                    ("include", "radio,tracks.artists,tracks.albums,albums.coverArt"),
                    ("countryCode", &sess.country_code),
                ],
            )
            .await?;
        Ok(parse_jsonapi_results(&doc).tracks)
    }

    /// Charge pistes, albums et playlists d'une page Explorer TIDAL
    /// (`pages/explore_new_music`, `pages/explore_top_music`, `pages/genre_*`, etc.)
    /// avec fallback sur l'API officielle `searchResults` / `search`.
    pub async fn explore_category(
        &self,
        page_paths: &[&str],
        fallback_query: &str,
        max_tracks: usize,
    ) -> Result<SearchResults> {
        let sess = self.ensure_session().await?;
        for path in page_paths {
            if let Ok(json) = self
                .auth
                .get_v1::<Value>(
                    path,
                    &[
                        ("countryCode", &sess.country_code),
                        ("deviceType", "DESKTOP"),
                        ("locale", "fr_FR"),
                    ],
                )
                .await
            {
                let res = extract_results_from_page_json(&json, max_tracks, 12, 8);
                if !res.tracks.is_empty() || !res.albums.is_empty() || !res.playlists.is_empty() {
                    return Ok(res);
                }
            }
        }
        if !fallback_query.is_empty() {
            return self.search(fallback_query, max_tracks.min(50)).await;
        }
        Ok(SearchResults::default())
    }

    /// Charge un mix personnalisé via l'endpoint officiel OpenAPI v2 (`THIRD_PARTY`) :
    /// `userDailyMixes`, `userDiscoveryMixes` ou `userNewReleaseMixes`.
    ///
    /// `include=items` ne renvoie que les pistes (sans artiste, album ni pochette) :
    /// on demande donc aussi leurs relations, avec repli sur `items` si le serveur
    /// refuse les inclusions imbriquées.
    pub async fn openapi_user_mix(&self, resource: &str) -> Result<SearchResults> {
        let sess = self.ensure_session().await?;
        let user_ref = if sess.user_id > 0 {
            sess.user_id.to_string()
        } else {
            "me".to_string()
        };
        let path = format!("{resource}/{user_ref}");
        const RICH_INCLUDE: &str = "items.artists,items.albums,items.albums.coverArt";
        if let Ok(doc) = self
            .auth
            .get_openapi_v2(&path, &[("include", RICH_INCLUDE), ("locale", "fr-FR")])
            .await
        {
            let rich = parse_jsonapi_results(&doc);
            if !rich.tracks.is_empty() {
                return Ok(rich);
            }
        }
        let doc: Value = self
            .auth
            .get_openapi_v2(&path, &[("include", "items"), ("locale", "fr-FR")])
            .await?;
        Ok(parse_jsonapi_results(&doc))
    }

    /// Charge les pistes d'un mix de collection via la page v1 `pages/mix`
    /// (repli quand l'OpenAPI v2 ne renvoie que la carte du mix, sans pistes).
    pub async fn mix_tracks(&self, mix_id: &str, max_tracks: usize) -> Result<Vec<TrackSummary>> {
        let sess = self.ensure_session().await?;
        let page: Value = self
            .auth
            .get_v1(
                "pages/mix",
                &[
                    ("mixId", mix_id),
                    ("deviceType", "BROWSER"),
                    ("countryCode", &sess.country_code),
                ],
            )
            .await?;
        Ok(extract_tracks_from_page_json(&page, max_tracks))
    }

    /// Assemble les cartes de mix personnalisés TIDAL : `userDailyMixes`, `userDiscoveryMixes`
    /// et `userNewReleaseMixes` (OpenAPI v2) + éventuels mixes de `pages/my_collection_my_mixes`.
    pub async fn user_mixes(&self) -> Result<Vec<MixSummary>> {
        let (daily, discovery, new_releases) = tokio::join!(
            self.openapi_user_mix("userDailyMixes"),
            self.openapi_user_mix("userDiscoveryMixes"),
            self.openapi_user_mix("userNewReleaseMixes"),
        );

        let mut mixes = Vec::new();
        let mut seen = std::collections::HashSet::new();
        if let Ok(res) = daily {
            push_mix_from_results(
                &mut mixes,
                &mut seen,
                "userDailyMixes",
                "Ma Découverte Quotidienne",
                "Sélection personnalisée du jour",
                res,
            );
        }
        if let Ok(res) = discovery {
            push_mix_from_results(
                &mut mixes,
                &mut seen,
                "userDiscoveryMixes",
                "Mes Mix du Jour",
                "Mix découverte TIDAL",
                res,
            );
        }
        if let Ok(res) = new_releases {
            push_mix_from_results(
                &mut mixes,
                &mut seen,
                "userNewReleaseMixes",
                "Mes Nouveautés",
                "Sorties récentes pour vous",
                res,
            );
        }

        let sess = self.ensure_session().await?;
        if let Ok(page) = self
            .auth
            .get_v1::<Value>(
                "pages/my_collection_my_mixes",
                &[
                    ("countryCode", &sess.country_code),
                    ("deviceType", "DESKTOP"),
                    ("locale", "fr_FR"),
                ],
            )
            .await
        {
            for mix in parse_my_mixes_page(&page) {
                if mixes.len() >= 8 {
                    break;
                }
                if seen.insert(mix.title.clone()) {
                    mixes.push(mix);
                }
            }
        }

        Ok(mixes)
    }

    /// Charge le hub **Explorer** natif TIDAL avec de vraies miniatures d'albums, playlists et genres,
    /// en combinant les endpoints officiels OpenAPI v2 (`userNewReleaseMixes`, `userDiscoveryMixes`,
    /// `userDailyMixes`, `searchResults`) et les pages catalogue TIDAL en parallèle.
    pub async fn explore_hub(&self) -> Result<ExploreHubData> {
        let sess = self.ensure_session().await?;
        let cc = sess.country_code.clone();

        let (new_mix_res, disc_mix_res, daily_mix_res, new_page, top_page, hires_page) = tokio::join!(
            self.openapi_user_mix("userNewReleaseMixes"),
            self.openapi_user_mix("userDiscoveryMixes"),
            self.openapi_user_mix("userDailyMixes"),
            self.fetch_page_rich("pages/explore_new_music", &cc, 30, 12, 8),
            self.fetch_page_rich("pages/explore_top_music", &cc, 30, 12, 8),
            self.fetch_page_rich("pages/hires", &cc, 30, 12, 8),
        );

        let mut hub = ExploreHubData::default();
        let mut seen_albums = std::collections::HashSet::new();
        let mut seen_playlists = std::collections::HashSet::new();
        let mut seen_tracks = std::collections::HashSet::new();

        let mut push_results = |res: SearchResults, is_new: bool| {
            for alb in res.albums {
                if alb.cover_id.is_some() && seen_albums.insert(alb.id) {
                    hub.spotlight_albums.push(alb);
                }
            }
            for pl in res.playlists {
                if pl.image_id.is_some() && seen_playlists.insert(pl.uuid.clone()) {
                    hub.featured_playlists.push(pl);
                }
            }
            for tr in res.tracks {
                if let Some(ref cid) = tr.cover_id {
                    let aid = tr.album_id.unwrap_or(0);
                    if aid > 0 && seen_albums.insert(aid) {
                        hub.spotlight_albums.push(AlbumSummary {
                            id: aid,
                            title: if tr.album_title.is_empty() {
                                tr.title.clone()
                            } else {
                                tr.album_title.clone()
                            },
                            artist_name: tr.artist_name.clone(),
                            artist_id: tr.artist_id,
                            cover_id: Some(cid.clone()),
                            release_date: None,
                            audio_quality: tr.audio_quality.clone(),
                            number_of_tracks: 0,
                        });
                    }
                }
                if seen_tracks.insert(tr.id) {
                    if is_new {
                        hub.new_release_tracks.push(tr);
                    } else {
                        hub.discovery_tracks.push(tr);
                    }
                }
            }
        };

        if let Ok(r) = new_mix_res {
            push_results(r, true);
        }
        if let Ok(r) = disc_mix_res {
            push_results(r, false);
        }
        if let Ok(r) = daily_mix_res {
            push_results(r, false);
        }

        let new_cover = first_cover_from_results(&new_page);
        let top_cover = first_cover_from_results(&top_page);
        let hires_cover = first_cover_from_results(&hires_page);

        push_results(new_page, true);
        push_results(top_page, false);
        push_results(hires_page, false);

        if let Some(c) = new_cover {
            hub.category_covers.insert("Nouveautés".into(), c);
        }
        if let Some(c) = top_cover {
            hub.category_covers.insert("Top 20".into(), c);
        }
        if let Some(c) = hires_cover {
            hub.category_covers.insert("Hi-Res · MAX".into(), c);
        }

        // Précharge en parallèle de vraies miniatures représentatives pour les genres & ambiances
        let genre_probes: &[(&str, &str)] = &[
            ("Ambiances", "chill ambient focus"),
            ("Hip-Hop", "hip hop rap"),
            ("Pop", "pop hits"),
            ("Électro", "electronic house"),
            ("Rock", "rock classics"),
            ("R&B / Soul", "r&b soul"),
            ("Jazz", "jazz audiophile"),
            ("Classique", "classical orchestra"),
            ("Français", "chanson francaise"),
            ("Afrobeats", "afrobeats amapiano"),
            ("Metal", "metal rock"),
            ("Indie", "indie alternative"),
            ("Bandes originales", "film soundtrack score"),
        ];

        let sem = Arc::new(Semaphore::new(4));
        let mut handles = Vec::with_capacity(genre_probes.len());
        for &(label, query) in genre_probes {
            let api = self.clone();
            let sem = Arc::clone(&sem);
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire_owned().await.ok()?;
                let res = api.search(query, 4).await.ok()?;
                let cover = first_cover_from_results(&res)?;
                Some((label.to_string(), cover, res))
            }));
        }

        for h in handles {
            if let Ok(Some((label, cover, res))) = h.await {
                hub.category_covers.insert(label, cover);
                // Complète les étagères d'albums/playlists si les pages dynamiques étaient vides
                if hub.spotlight_albums.len() < 12 {
                    for alb in res.albums {
                        if alb.cover_id.is_some() && seen_albums.insert(alb.id) {
                            hub.spotlight_albums.push(alb);
                            if hub.spotlight_albums.len() >= 12 {
                                break;
                            }
                        }
                    }
                }
                if hub.featured_playlists.len() < 8 {
                    for pl in res.playlists {
                        if pl.image_id.is_some() && seen_playlists.insert(pl.uuid.clone()) {
                            hub.featured_playlists.push(pl);
                            if hub.featured_playlists.len() >= 8 {
                                break;
                            }
                        }
                    }
                }
            }
        }

        hub.spotlight_albums.truncate(12);
        hub.featured_playlists.truncate(8);
        hub.new_release_tracks.truncate(30);
        hub.discovery_tracks.truncate(30);

        // Assure des miniatures pour les 4 cartes phares même si un endpoint page n'a rien renvoyé
        let fallback_covers: Vec<String> = hub
            .spotlight_albums
            .iter()
            .filter_map(|a| a.cover_id.clone())
            .chain(hub.featured_playlists.iter().filter_map(|p| p.image_id.clone()))
            .collect();
        for (idx, key) in ["Nouveautés", "Top 20", "Hi-Res · MAX", "Ambiances"]
            .iter()
            .enumerate()
        {
            if !hub.category_covers.contains_key(*key) {
                if let Some(c) = fallback_covers.get(idx).cloned() {
                    hub.category_covers.insert((*key).to_string(), c);
                }
            }
        }

        Ok(hub)
    }

    async fn fetch_page_rich(
        &self,
        page_path: &str,
        country: &str,
        max_tracks: usize,
        max_albums: usize,
        max_playlists: usize,
    ) -> SearchResults {
        match self
            .auth
            .get_v1::<Value>(
                page_path,
                &[
                    ("countryCode", country),
                    ("deviceType", "DESKTOP"),
                    ("locale", "fr_FR"),
                ],
            )
            .await
        {
            Ok(json) => extract_results_from_page_json(&json, max_tracks, max_albums, max_playlists),
            Err(_) => SearchResults::default(),
        }
    }

    /// Recherche globale (titres, albums, playlists, artistes).
    /// Essaie l'endpoint catalogue v1 et bascule sur l'OpenAPI v2 officielle `/searchResults` si besoin.
    pub async fn search(&self, query: &str, limit: usize) -> Result<SearchResults> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(SearchResults::default());
        }
        let sess = self.ensure_session().await?;
        let limit_s = limit.clamp(1, 50).to_string();
        match self
            .auth
            .get_v1::<RawSearchResponse>(
                "search",
                &[
                    ("query", trimmed),
                    ("limit", &limit_s),
                    ("offset", "0"),
                    ("types", "TRACKS,ALBUMS,PLAYLISTS,ARTISTS"),
                    ("countryCode", &sess.country_code),
                ],
            )
            .await
        {
            Ok(raw) => Ok(raw.into_results()),
            Err(v1_err) => {
                // Repli officiel OpenAPI v2 (`GET https://openapi.tidal.com/v2/searchResults`)
                match self
                    .auth
                    .get_openapi_v2::<Value>(
                        "searchResults",
                        &[
                            ("filter[query]", trimmed),
                            ("countryCode", &sess.country_code),
                            ("include", "albums,artists,playlists,topHits,tracks"),
                        ],
                    )
                    .await
                {
                    Ok(doc) => Ok(parse_jsonapi_results(&doc)),
                    Err(_) => Err(ApiError::from(v1_err)),
                }
            }
        }
    }

    /// Suggestions instantanées d'autocomplétion : interroge la v1 (objets complets :
    /// artiste, album, pochette) puis l'OpenAPI v2 officielle (`searchSuggestions`
    /// `include=directHits`), fusionne, déduplique et plafonne (6 requêtes,
    /// 4 résultats directs par catégorie).
    pub async fn search_suggestions(&self, query: &str) -> Result<SearchSuggestions> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(SearchSuggestions::default());
        }
        let sess = self.ensure_session().await?;
        let mut out = SearchSuggestions::default();

        if let Ok(doc) = self
            .auth
            .get_v1::<Value>(
                "search/suggestions",
                &[
                    ("query", trimmed),
                    ("limit", "10"),
                    ("types", "TRACKS,ALBUMS,ARTISTS"),
                    ("countryCode", &sess.country_code),
                ],
            )
            .await
        {
            merge_suggestions(&mut out, parse_search_suggestions(&doc));
        }

        if let Ok(doc) = self
            .auth
            .get_openapi_v2::<Value>(
                "searchSuggestions",
                &[
                    ("filter[query]", trimmed),
                    ("include", "directHits"),
                    ("countryCode", &sess.country_code),
                ],
            )
            .await
        {
            merge_suggestions(&mut out, parse_search_suggestions(&doc));
        }

        out.queries.truncate(6);
        out.tracks.truncate(4);
        out.albums.truncate(4);
        out.artists.truncate(4);
        Ok(out)
    }

    /// Récupère une sélection de titres mis en avant Hi-Res / Découverte
    /// (OpenAPI v2 `userDiscoveryMixes` / `userDailyMixes` + pages Hi-Res).
    pub async fn hires_featured_tracks(&self, max_tracks: usize) -> Result<Vec<TrackSummary>> {
        let sess = self.ensure_session().await?;
        // 1. Essaie d'abord les endpoints officiels OpenAPI v2 personnalisés
        for mix_ep in ["userDiscoveryMixes", "userDailyMixes", "userNewReleaseMixes"] {
            if let Ok(res) = self.openapi_user_mix(mix_ep).await {
                if !res.tracks.is_empty() {
                    let mut tracks = res.tracks;
                    tracks.truncate(max_tracks);
                    return Ok(tracks);
                }
            }
        }
        // 2. Repli sur les pages éditoriales Hi-Res / Top / Nouveautés
        for page_path in ["pages/hires", "pages/explore_top_music", "pages/explore_new_music"] {
            if let Ok(json) = self
                .auth
                .get_v1::<Value>(
                    page_path,
                    &[
                        ("countryCode", &sess.country_code),
                        ("deviceType", "DESKTOP"),
                    ],
                )
                .await
            {
                let tracks = extract_tracks_from_page_json(&json, max_tracks);
                if !tracks.is_empty() {
                    return Ok(tracks);
                }
            }
        }
        Ok(Vec::new())
    }

    /// Récupère les identifiants des albums favoris de l'utilisateur.
    pub async fn favorite_album_ids(&self) -> Result<Vec<u64>> {
        let sess = self.ensure_session().await?;
        let path = format!("users/{}/favorites/ids", sess.user_id);
        if let Ok(val) = self
            .auth
            .get_v1::<Value>(&path, &[("countryCode", &sess.country_code)])
            .await
        {
            if let Some(ids) = parse_v1_favorite_album_ids(&val) {
                return Ok(ids);
            }
        }
        let oa_path = format!("userCollectionAlbums/{}/relationships/items", sess.user_id);
        if let Ok(doc) = self
            .auth
            .get_openapi_v2::<Value>(&oa_path, &[("countryCode", &sess.country_code)])
            .await
        {
            if let Some(ids) = parse_openapi_collection_album_ids(&doc) {
                return Ok(ids);
            }
        }
        Ok(Vec::new())
    }

    /// Charge les albums favoris de l'utilisateur (`v1 /users/{id}/favorites/albums`,
    /// repli OpenAPI v2 `userCollectionAlbums/{id}/relationships/items`).
    pub async fn favorite_albums(&self, max_items: usize) -> Result<Vec<AlbumSummary>> {
        let sess = self.ensure_session().await?;
        let path = format!("users/{}/favorites/albums", sess.user_id);
        let country = sess.country_code.clone();
        let auth = Arc::clone(&self.auth);

        let v1_pages = drain_pages(50, max_items, 4, move |offset, limit| {
            let auth = Arc::clone(&auth);
            let path = path.clone();
            let country = country.clone();
            async move {
                let offset_s = offset.to_string();
                let limit_s = limit.to_string();
                auth.get_v1::<PagedResponse<Value>>(
                    &path,
                    &[
                        ("limit", &limit_s),
                        ("offset", &offset_s),
                        ("countryCode", &country),
                    ],
                )
                .await
                .map_err(ApiError::from)
            }
        })
        .await;

        let mut albums = match v1_pages {
            Ok(items) => parse_favorite_albums_v1(&Value::Array(items)),
            Err(_) => Vec::new(),
        };
        if albums.is_empty() {
            let oa_path = format!("userCollectionAlbums/{}/relationships/items", sess.user_id);
            if let Ok(doc) = self
                .auth
                .get_openapi_v2::<Value>(
                    &oa_path,
                    &[
                        ("include", "items,albums.artists,albums.coverArt"),
                        ("countryCode", &sess.country_code),
                    ],
                )
                .await
            {
                albums = parse_jsonapi_results(&doc).albums;
            }
        }
        albums.truncate(max_items);
        Ok(albums)
    }

    /// Charge les artistes favoris de l'utilisateur (`v1 /users/{id}/favorites/artists`,
    /// repli OpenAPI v2 `userCollectionArtists/{id}/relationships/items`).
    pub async fn favorite_artists(&self, max_items: usize) -> Result<Vec<ArtistSummary>> {
        let sess = self.ensure_session().await?;
        let path = format!("users/{}/favorites/artists", sess.user_id);
        let country = sess.country_code.clone();
        let auth = Arc::clone(&self.auth);

        let v1_pages = drain_pages(50, max_items, 4, move |offset, limit| {
            let auth = Arc::clone(&auth);
            let path = path.clone();
            let country = country.clone();
            async move {
                let offset_s = offset.to_string();
                let limit_s = limit.to_string();
                auth.get_v1::<PagedResponse<Value>>(
                    &path,
                    &[
                        ("limit", &limit_s),
                        ("offset", &offset_s),
                        ("countryCode", &country),
                    ],
                )
                .await
                .map_err(ApiError::from)
            }
        })
        .await;

        let mut artists = match v1_pages {
            Ok(items) => parse_favorite_artists_v1(&Value::Array(items)),
            Err(_) => Vec::new(),
        };
        if artists.is_empty() {
            let oa_path = format!("userCollectionArtists/{}/relationships/items", sess.user_id);
            if let Ok(doc) = self
                .auth
                .get_openapi_v2::<Value>(
                    &oa_path,
                    &[
                        ("include", "items,artists.profileArt"),
                        ("countryCode", &sess.country_code),
                    ],
                )
                .await
            {
                artists = parse_jsonapi_results(&doc).artists;
            }
        }
        artists.truncate(max_items);
        Ok(artists)
    }

    /// Ajoute un titre aux favoris du compte TIDAL.
    pub async fn add_favorite_track(&self, track_id: u64) -> Result<()> {
        let sess = self.ensure_session().await?;
        let path = format!("users/{}/favorites/tracks", sess.user_id);
        let id_str = track_id.to_string();
        match self
            .auth
            .post_v1_form(
                &path,
                &[("countryCode", &sess.country_code)],
                &[("trackIds", &id_str), ("onArtifactNotFound", "FAIL")],
                None,
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(v1_err) => {
                let oa_path = format!("userCollectionTracks/{}/relationships/items", sess.user_id);
                let body = serde_json::json!({
                    "data": [{ "id": id_str, "type": "tracks" }]
                });
                match self
                    .auth
                    .post_openapi_v2(&oa_path, &[("countryCode", &sess.country_code)], &body)
                    .await
                {
                    Ok(_) => Ok(()),
                    Err(_) => Err(ApiError::from(v1_err)),
                }
            }
        }
    }

    /// Retire un titre des favoris du compte TIDAL.
    pub async fn remove_favorite_track(&self, track_id: u64) -> Result<()> {
        let sess = self.ensure_session().await?;
        let path = format!("users/{}/favorites/tracks/{track_id}", sess.user_id);
        let id_str = track_id.to_string();
        match self
            .auth
            .delete_v1(&path, &[("countryCode", &sess.country_code)], None)
            .await
        {
            Ok(_) => Ok(()),
            Err(v1_err) => {
                let oa_path = format!("userCollectionTracks/{}/relationships/items", sess.user_id);
                let body = serde_json::json!({
                    "data": [{ "id": id_str, "type": "tracks" }]
                });
                match self
                    .auth
                    .delete_openapi_v2(&oa_path, &[("countryCode", &sess.country_code)], Some(&body))
                    .await
                {
                    Ok(_) => Ok(()),
                    Err(_) => Err(ApiError::from(v1_err)),
                }
            }
        }
    }

    /// Sauvegarde un album dans les favoris du compte TIDAL.
    pub async fn add_favorite_album(&self, album_id: u64) -> Result<()> {
        let sess = self.ensure_session().await?;
        let path = format!("users/{}/favorites/albums", sess.user_id);
        let id_str = album_id.to_string();
        match self
            .auth
            .post_v1_form(
                &path,
                &[("countryCode", &sess.country_code)],
                &[("albumIds", &id_str), ("onArtifactNotFound", "FAIL")],
                None,
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(v1_err) => {
                let oa_path = format!("userCollectionAlbums/{}/relationships/items", sess.user_id);
                let body = serde_json::json!({
                    "data": [{ "id": id_str, "type": "albums" }]
                });
                match self
                    .auth
                    .post_openapi_v2(&oa_path, &[("countryCode", &sess.country_code)], &body)
                    .await
                {
                    Ok(_) => Ok(()),
                    Err(_) => Err(ApiError::from(v1_err)),
                }
            }
        }
    }

    /// Retire un album des favoris du compte TIDAL.
    pub async fn remove_favorite_album(&self, album_id: u64) -> Result<()> {
        let sess = self.ensure_session().await?;
        let path = format!("users/{}/favorites/albums/{album_id}", sess.user_id);
        let id_str = album_id.to_string();
        match self
            .auth
            .delete_v1(&path, &[("countryCode", &sess.country_code)], None)
            .await
        {
            Ok(_) => Ok(()),
            Err(v1_err) => {
                let oa_path = format!("userCollectionAlbums/{}/relationships/items", sess.user_id);
                let body = serde_json::json!({
                    "data": [{ "id": id_str, "type": "albums" }]
                });
                match self
                    .auth
                    .delete_openapi_v2(&oa_path, &[("countryCode", &sess.country_code)], Some(&body))
                    .await
                {
                    Ok(_) => Ok(()),
                    Err(_) => Err(ApiError::from(v1_err)),
                }
            }
        }
    }

    /// Ajoute un artiste aux favoris du compte TIDAL.
    pub async fn add_favorite_artist(&self, artist_id: u64) -> Result<()> {
        let sess = self.ensure_session().await?;
        let path = format!("users/{}/favorites/artists", sess.user_id);
        let id_str = artist_id.to_string();
        match self
            .auth
            .post_v1_form(
                &path,
                &[("countryCode", &sess.country_code)],
                &[("artistIds", &id_str), ("onArtifactNotFound", "FAIL")],
                None,
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(v1_err) => {
                let oa_path = format!("userCollectionArtists/{}/relationships/items", sess.user_id);
                let body = serde_json::json!({
                    "data": [{ "id": id_str, "type": "artists" }]
                });
                match self
                    .auth
                    .post_openapi_v2(&oa_path, &[("countryCode", &sess.country_code)], &body)
                    .await
                {
                    Ok(_) => Ok(()),
                    Err(_) => Err(ApiError::from(v1_err)),
                }
            }
        }
    }

    /// Retire un artiste des favoris du compte TIDAL.
    pub async fn remove_favorite_artist(&self, artist_id: u64) -> Result<()> {
        let sess = self.ensure_session().await?;
        let path = format!("users/{}/favorites/artists/{artist_id}", sess.user_id);
        let id_str = artist_id.to_string();
        match self
            .auth
            .delete_v1(&path, &[("countryCode", &sess.country_code)], None)
            .await
        {
            Ok(_) => Ok(()),
            Err(v1_err) => {
                let oa_path = format!("userCollectionArtists/{}/relationships/items", sess.user_id);
                let body = serde_json::json!({
                    "data": [{ "id": id_str, "type": "artists" }]
                });
                match self
                    .auth
                    .delete_openapi_v2(&oa_path, &[("countryCode", &sess.country_code)], Some(&body))
                    .await
                {
                    Ok(_) => Ok(()),
                    Err(_) => Err(ApiError::from(v1_err)),
                }
            }
        }
    }

    /// Crée une nouvelle playlist utilisateur sur le compte TIDAL.
    pub async fn create_playlist(
        &self,
        title: &str,
        description: &str,
    ) -> Result<PlaylistSummary> {
        let sess = self.ensure_session().await?;
        let path = format!("users/{}/playlists", sess.user_id);
        match self
            .auth
            .post_v1_form(
                &path,
                &[("countryCode", &sess.country_code)],
                &[("title", title), ("description", description)],
                None,
            )
            .await
        {
            Ok(body) => parse_created_playlist_response(&body, title, description).ok_or_else(|| {
                ApiError::Unexpected(
                    "création de playlist : réponse TIDAL v1 sans identifiant".into(),
                )
            }),
            Err(v1_err) => {
                let payload = serde_json::json!({
                    "data": {
                        "type": "playlists",
                        "attributes": {
                            "name": title,
                            "description": description,
                            "accessType": "UNLISTED"
                        }
                    }
                });
                match self
                    .auth
                    .post_openapi_v2("playlists", &[("countryCode", &sess.country_code)], &payload)
                    .await
                {
                    Ok(body) => {
                        parse_jsonapi_created_playlist(&body, title, description).ok_or_else(|| {
                            ApiError::Unexpected(
                                "création de playlist : réponse TIDAL v2 sans identifiant".into(),
                            )
                        })
                    }
                    Err(_) => Err(ApiError::from(v1_err)),
                }
            }
        }
    }

    /// Supprime une playlist utilisateur sur le compte TIDAL.
    pub async fn delete_playlist(&self, playlist_uuid: &str) -> Result<()> {
        let sess = self.ensure_session().await?;
        let path = format!("playlists/{playlist_uuid}");
        let etag = self
            .auth
            .get_v1_etag(&path, &[("countryCode", &sess.country_code)])
            .await
            .ok()
            .flatten();
        match self
            .auth
            .delete_v1(
                &path,
                &[("countryCode", &sess.country_code)],
                etag.as_deref(),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(v1_err) => {
                match self
                    .auth
                    .delete_openapi_v2(&path, &[("countryCode", &sess.country_code)], None)
                    .await
                {
                    Ok(_) => Ok(()),
                    Err(_) => Err(ApiError::from(v1_err)),
                }
            }
        }
    }

    /// Ajoute une ou plusieurs pistes à une playlist TIDAL.
    pub async fn add_tracks_to_playlist(
        &self,
        playlist_uuid: &str,
        track_ids: &[u64],
    ) -> Result<()> {
        if track_ids.is_empty() {
            return Ok(());
        }
        let sess = self.ensure_session().await?;
        let pl_path = format!("playlists/{playlist_uuid}");
        let etag = self
            .auth
            .get_v1_etag(&pl_path, &[("countryCode", &sess.country_code)])
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "*".into());
        let items_path = format!("playlists/{playlist_uuid}/items");
        let ids_csv = track_ids
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",");

        match self
            .auth
            .post_v1_form(
                &items_path,
                &[("countryCode", &sess.country_code)],
                &[
                    ("trackIds", &ids_csv),
                    ("onArtifactNotFound", "SKIP"),
                    ("onDupes", "SKIP"),
                ],
                Some(&etag),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(v1_err) => {
                let oa_path = format!("playlists/{playlist_uuid}/relationships/items");
                let data: Vec<Value> = track_ids
                    .iter()
                    .map(|id| serde_json::json!({ "id": id.to_string(), "type": "tracks" }))
                    .collect();
                let payload = serde_json::json!({ "data": data });
                match self
                    .auth
                    .post_openapi_v2(&oa_path, &[("countryCode", &sess.country_code)], &payload)
                    .await
                {
                    Ok(_) => Ok(()),
                    Err(_) => Err(ApiError::from(v1_err)),
                }
            }
        }
    }

    /// Retire une piste d'une playlist TIDAL à l'index donné.
    pub async fn remove_track_from_playlist(
        &self,
        playlist_uuid: &str,
        index: usize,
        track_id: u64,
    ) -> Result<()> {
        let sess = self.ensure_session().await?;
        let pl_path = format!("playlists/{playlist_uuid}");
        let etag = self
            .auth
            .get_v1_etag(&pl_path, &[("countryCode", &sess.country_code)])
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "*".into());
        let item_path = format!("playlists/{playlist_uuid}/items/{index}");
        match self
            .auth
            .delete_v1(
                &item_path,
                &[("countryCode", &sess.country_code)],
                Some(&etag),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(v1_err) => {
                let oa_path = format!("playlists/{playlist_uuid}/relationships/items");
                let payload = serde_json::json!({
                    "data": [{ "id": track_id.to_string(), "type": "tracks" }]
                });
                match self
                    .auth
                    .delete_openapi_v2(
                        &oa_path,
                        &[("countryCode", &sess.country_code)],
                        Some(&payload),
                    )
                    .await
                {
                    Ok(_) => Ok(()),
                    Err(_) => Err(ApiError::from(v1_err)),
                }
            }
        }
    }

    /// Charge les paroles d'un titre TIDAL (texte brut + lignes synchronisées LRC).
    ///
    /// Interroge d'abord `GET /v1/tracks/{id}/lyrics`, puis se replie sur l'OpenAPI v2
    /// officielle `tracks/{id}/relationships/lyrics?include=lyrics`. Un 404 signifie
    /// simplement que le titre n'a pas de paroles : `Ok(None)`.
    pub async fn track_lyrics(&self, track_id: u64) -> Result<Option<TrackLyrics>> {
        let sess = self.ensure_session().await?;
        let path = format!("tracks/{track_id}/lyrics");
        let mut v1_error = None;
        match self
            .auth
            .get_v1::<Value>(&path, &[("countryCode", &sess.country_code)])
            .await
        {
            Ok(value) => {
                if let Some(lyrics) = parse_track_lyrics(track_id, &value) {
                    return Ok(Some(lyrics));
                }
            }
            Err(err) => v1_error = Some(err),
        }

        let oa_path = format!("tracks/{track_id}/relationships/lyrics");
        match self
            .auth
            .get_openapi_v2::<Value>(
                &oa_path,
                &[
                    ("include", "lyrics"),
                    ("countryCode", &sess.country_code),
                ],
            )
            .await
        {
            Ok(doc) => Ok(parse_track_lyrics(track_id, &doc)),
            Err(_) => match v1_error {
                Some(err) if !is_not_found(&err) => Err(ApiError::from(err)),
                _ => Ok(None),
            },
        }
    }

    /// Charge les crédits d'un titre TIDAL (`GET /v1/tracks/{id}/credits?includeContributors=true`),
    /// regroupés par rôle et dédoublonnés. Un 404 renvoie des crédits vides.
    pub async fn track_credits(&self, track_id: u64) -> Result<TrackCredits> {
        let sess = self.ensure_session().await?;
        let path = format!("tracks/{track_id}/credits");
        match self
            .auth
            .get_v1::<Value>(
                &path,
                &[
                    ("includeContributors", "true"),
                    ("countryCode", &sess.country_code),
                ],
            )
            .await
        {
            Ok(value) => Ok(parse_track_credits(track_id, &value)),
            Err(AuthError::Api { status: 404, .. }) => Ok(TrackCredits {
                track_id,
                groups: Vec::new(),
            }),
            Err(err) => Err(err.into()),
        }
    }

    /// Charge la radio d'un titre (v1 `tracks/{id}/radio`, repli OpenAPI v2
    /// `relationships/radio` puis `relationships/similarTracks`).
    pub async fn track_radio(&self, track_id: u64) -> Result<Vec<TrackSummary>> {
        let sess = self.ensure_session().await?;
        let path = format!("tracks/{track_id}/radio");
        if let Ok(page) = self
            .auth
            .get_v1::<Value>(
                &path,
                &[
                    ("limit", "40"),
                    ("offset", "0"),
                    ("countryCode", &sess.country_code),
                ],
            )
            .await
        {
            let tracks = parse_radio_tracks(&page);
            if !tracks.is_empty() {
                return Ok(tracks);
            }
        }

        for (relationship, include) in [
            (
                "radio",
                "radio,tracks.artists,tracks.albums,albums.coverArt",
            ),
            (
                "similarTracks",
                "similarTracks,tracks.artists,tracks.albums,albums.coverArt",
            ),
        ] {
            let oa_path = format!("tracks/{track_id}/relationships/{relationship}");
            if let Ok(doc) = self
                .auth
                .get_openapi_v2::<Value>(
                    &oa_path,
                    &[("include", include), ("countryCode", &sess.country_code)],
                )
                .await
            {
                let tracks = parse_jsonapi_results(&doc).tracks;
                if !tracks.is_empty() {
                    return Ok(tracks);
                }
            }
        }
        Ok(Vec::new())
    }
}

fn is_not_found(err: &AuthError) -> bool {
    matches!(err, AuthError::Api { status: 404, .. })
}

/// Extrait le premier champ texte non vide parmi `keys`.
fn first_non_empty_str(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    })
}

/// Parse une réponse `GET /v1/tracks/{id}/lyrics` (ou la ressource `lyrics`
/// incluse d'un document JSON:API OpenAPI v2) vers [`TrackLyrics`].
fn parse_track_lyrics(track_id: u64, value: &Value) -> Option<TrackLyrics> {
    let mut candidates: Vec<&Value> = vec![value];
    if let Some(data) = value.get("data") {
        candidates.push(data);
    }
    if let Some(included) = value.get("included").and_then(Value::as_array) {
        for resource in included {
            if resource.get("type").and_then(Value::as_str) == Some("lyrics") {
                candidates.push(resource);
            }
        }
    }
    // Ajoute les `attributes` JSON:API rencontrés (racine, data, ressources incluses).
    let mut idx = 0;
    while idx < candidates.len() {
        if let Some(attrs) = candidates[idx].get("attributes") {
            candidates.push(attrs);
        }
        idx += 1;
    }

    let mut plain_text = None;
    let mut subtitles = None;
    let mut provider = None;
    for candidate in candidates {
        if plain_text.is_none() {
            plain_text = first_non_empty_str(
                candidate,
                &["lyrics", "plainLyrics", "plainText", "text"],
            );
        }
        if subtitles.is_none() {
            subtitles = first_non_empty_str(candidate, &["subtitles", "syncedLyrics", "synced"]);
        }
        if provider.is_none() {
            provider = first_non_empty_str(candidate, &["lyricsProvider", "provider"]);
        }
    }

    let synced_lines = match subtitles.as_deref().map(parse_synced_lyrics) {
        Some(lines) if !lines.is_empty() => lines,
        // Certaines réponses OpenAPI v2 renvoient le LRC directement dans `text`.
        _ => plain_text
            .as_deref()
            .map(parse_synced_lyrics)
            .unwrap_or_default(),
    };
    if plain_text.is_none() && synced_lines.is_empty() {
        return None;
    }
    let plain_text = plain_text.or_else(|| {
        (!synced_lines.is_empty()).then(|| {
            synced_lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
    });

    Some(TrackLyrics {
        track_id,
        plain_text,
        synced_lines,
        provider,
    })
}

/// Parse une réponse `GET /v1/tracks/{id}/credits?includeContributors=true`
/// (tableau direct ou enveloppé `items` / `contributors` / `data`).
fn parse_track_credits(track_id: u64, value: &Value) -> TrackCredits {
    let mut groups: Vec<TrackCreditGroup> = Vec::new();
    let entries = value
        .as_array()
        .or_else(|| value.get("items").and_then(Value::as_array))
        .or_else(|| value.get("contributors").and_then(Value::as_array))
        .or_else(|| value.get("data").and_then(Value::as_array));

    let Some(entries) = entries else {
        return TrackCredits { track_id, groups };
    };

    for entry in entries {
        let role = entry
            .get("type")
            .or_else(|| entry.get("role"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if role.is_empty() {
            continue;
        }
        let Some(contributors) = entry.get("contributors").and_then(Value::as_array) else {
            continue;
        };
        let mut names: Vec<String> = Vec::new();
        for contributor in contributors {
            let name = contributor
                .as_str()
                .or_else(|| contributor.get("name").and_then(Value::as_str))
                .unwrap_or_default()
                .trim();
            if !name.is_empty() && !names.iter().any(|known| known.eq_ignore_ascii_case(name)) {
                names.push(name.to_string());
            }
        }
        if names.is_empty() {
            continue;
        }
        match groups
            .iter_mut()
            .find(|group| group.role.eq_ignore_ascii_case(role))
        {
            Some(group) => {
                for name in names {
                    if !group
                        .contributors
                        .iter()
                        .any(|known| known.eq_ignore_ascii_case(&name))
                    {
                        group.contributors.push(name);
                    }
                }
            }
            None => groups.push(TrackCreditGroup {
                role: role.to_string(),
                contributors: names,
            }),
        }
    }

    TrackCredits { track_id, groups }
}

/// Parse une page `v1 /tracks/{id}/radio` : tableau direct ou `items` d'objets
/// `RawTrack` nus ou enveloppés `{ "item": ... }`, dédoublonnés par identifiant.
fn parse_radio_tracks(value: &Value) -> Vec<TrackSummary> {
    let mut tracks = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(items) = value.get("items").and_then(Value::as_array) {
        extend_unique_tracks(items, &mut tracks, &mut seen);
    } else if let Some(items) = value.as_array() {
        extend_unique_tracks(items, &mut tracks, &mut seen);
    } else {
        for key in ["radio", "similarTracks", "tracks"] {
            if let Some(items) = value
                .get(key)
                .and_then(|node| node.get("items"))
                .and_then(Value::as_array)
            {
                extend_unique_tracks(items, &mut tracks, &mut seen);
            }
        }
    }
    tracks
}

fn extend_unique_tracks(
    items: &[Value],
    tracks: &mut Vec<TrackSummary>,
    seen: &mut std::collections::HashSet<u64>,
) {
    for item in items {
        let payload = item.get("item").unwrap_or(item);
        if let Ok(raw) = serde_json::from_value::<RawTrack>(payload.clone()) {
            if let Some(track) = raw.into_summary() {
                if seen.insert(track.id) {
                    tracks.push(track);
                }
            }
        }
    }
}

fn parse_v1_favorite_album_ids(value: &Value) -> Option<Vec<u64>> {
    let arr = value.get("ALBUM").and_then(Value::as_array)?;
    Some(
        arr.iter()
            .filter_map(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
            })
            .collect(),
    )
}

fn parse_openapi_collection_album_ids(doc: &Value) -> Option<Vec<u64>> {
    let arr = doc.get("data").and_then(Value::as_array)?;
    Some(
        arr.iter()
            .filter_map(|item| item.get("id").and_then(Value::as_str))
            .filter_map(|s| s.parse::<u64>().ok())
            .collect(),
    )
}

/// Parse la réponse `POST /v1/users/{id}/playlists` (objet playlist v1, ou au minimum `{ "uuid": ... }`).
fn parse_created_playlist_response(
    body: &str,
    fallback_title: &str,
    fallback_description: &str,
) -> Option<PlaylistSummary> {
    if let Ok(raw) = serde_json::from_str::<models::RawPlaylist>(body) {
        if let Some(summary) = raw.into_summary() {
            return Some(summary);
        }
    }
    let val: Value = serde_json::from_str(body).ok()?;
    let uuid = val.get("uuid").and_then(Value::as_str)?.trim();
    if uuid.is_empty() {
        return None;
    }
    Some(PlaylistSummary {
        uuid: uuid.to_string(),
        title: fallback_title.to_string(),
        description: (!fallback_description.is_empty()).then(|| fallback_description.to_string()),
        creator: "Moi".into(),
        number_of_tracks: 0,
        image_id: None,
    })
}

/// Parse la réponse JSON:API `POST https://openapi.tidal.com/v2/playlists`.
fn parse_jsonapi_created_playlist(
    body: &str,
    fallback_title: &str,
    fallback_description: &str,
) -> Option<PlaylistSummary> {
    let val: Value = serde_json::from_str(body).ok()?;
    let uuid = val.pointer("/data/id").and_then(Value::as_str)?.trim();
    if uuid.is_empty() {
        return None;
    }
    let title = val
        .pointer("/data/attributes/name")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(fallback_title);
    let description = val
        .pointer("/data/attributes/description")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(fallback_description);
    Some(PlaylistSummary {
        uuid: uuid.to_string(),
        title: title.to_string(),
        description: (!description.is_empty()).then(|| description.to_string()),
        creator: "Moi".into(),
        number_of_tracks: 0,
        image_id: None,
    })
}

fn first_cover_from_results(res: &SearchResults) -> Option<String> {
    res.albums
        .iter()
        .find_map(|a| a.cover_id.clone())
        .or_else(|| res.playlists.iter().find_map(|p| p.image_id.clone()))
        .or_else(|| res.tracks.iter().find_map(|t| t.cover_id.clone()))
}

/// Parse une page `v1 /users/{id}/favorites/albums` (objets directs ou enveloppés `{ "item": ... }`).
fn parse_favorite_albums_v1(doc: &Value) -> Vec<AlbumSummary> {
    if let Some(items) = doc.get("items").and_then(Value::as_array) {
        return parse_album_items(items);
    }
    doc.as_array()
        .map(|items| parse_album_items(items))
        .unwrap_or_default()
}

/// Parse une page `v1 /users/{id}/favorites/artists` (objets directs ou enveloppés `{ "item": ... }`).
fn parse_favorite_artists_v1(doc: &Value) -> Vec<ArtistSummary> {
    if let Some(items) = doc.get("items").and_then(Value::as_array) {
        return parse_artist_items(items);
    }
    doc.as_array()
        .map(|items| parse_artist_items(items))
        .unwrap_or_default()
}

fn parse_album_items(items: &[Value]) -> Vec<AlbumSummary> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in items {
        let payload = item.get("item").unwrap_or(item);
        if let Ok(raw) = serde_json::from_value::<RawAlbum>(payload.clone()) {
            if let Some(album) = raw.into_summary() {
                if seen.insert(album.id) {
                    out.push(album);
                }
            }
        }
    }
    out
}

fn parse_artist_items(items: &[Value]) -> Vec<ArtistSummary> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in items {
        let payload = item.get("item").unwrap_or(item);
        if let Ok(raw) = serde_json::from_value::<RawArtist>(payload.clone()) {
            if let Some(artist) = raw.into_summary() {
                if seen.insert(artist.id) {
                    out.push(artist);
                }
            }
        }
    }
    out
}

/// Nettoie une biographie TIDAL : retire les liens `[wimpLink artistId="1"]Nom[/wimpLink]`,
/// les balises HTML (`<br/>`, `<i>`, ...), décode les entités courantes et normalise les espaces.
pub fn clean_tidal_bio(raw: &str) -> String {
    let mut without_links = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('[') {
        without_links.push_str(&rest[..start]);
        let Some(relative_end) = rest[start..].find(']') else {
            without_links.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let end = start + relative_end;
        let tag = rest[start + 1..end].trim().to_ascii_lowercase();
        if tag.starts_with("wimp") || tag.starts_with("/wimp") {
            rest = &rest[end + 1..];
        } else {
            without_links.push('[');
            rest = &rest[start + 1..];
        }
    }
    without_links.push_str(rest);

    let mut without_html = String::with_capacity(without_links.len());
    let mut in_tag = false;
    for ch in without_links.chars() {
        match ch {
            '<' => {
                in_tag = true;
                without_html.push(' ');
            }
            '>' => in_tag = false,
            _ if !in_tag => without_html.push(ch),
            _ => {}
        }
    }

    without_html
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse les deux formes de suggestions TIDAL : v1 (`suggestions` + `directHits`)
/// et OpenAPI v2 (`data.attributes.suggestions` + ressources `included`).
fn parse_search_suggestions(doc: &Value) -> SearchSuggestions {
    let mut out = SearchSuggestions::default();

    collect_suggestion_queries(doc.get("suggestions"), &mut out.queries);
    collect_suggestion_queries(
        doc.pointer("/data/attributes/suggestions"),
        &mut out.queries,
    );

    if let Some(hits) = doc.get("directHits").and_then(Value::as_array) {
        for hit in hits {
            let payload = hit.get("value").unwrap_or(hit);
            let kind = hit
                .get("type")
                .or_else(|| payload.get("type"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_uppercase();
            match kind.as_str() {
                "TRACK" | "TRACKS" => {
                    if let Ok(raw) = serde_json::from_value::<RawTrack>(payload.clone()) {
                        if let Some(track) = raw.into_summary() {
                            push_unique_track(&mut out.tracks, track);
                        }
                    }
                }
                "ALBUM" | "ALBUMS" => {
                    if let Ok(raw) = serde_json::from_value::<RawAlbum>(payload.clone()) {
                        if let Some(album) = raw.into_summary() {
                            push_unique_album(&mut out.albums, album);
                        }
                    }
                }
                "ARTIST" | "ARTISTS" => {
                    if let Ok(raw) = serde_json::from_value::<RawArtist>(payload.clone()) {
                        if let Some(artist) = raw.into_summary() {
                            push_unique_artist(&mut out.artists, artist);
                        }
                    }
                }
                _ => {
                    if payload.get("duration").is_some() {
                        if let Ok(raw) = serde_json::from_value::<RawTrack>(payload.clone()) {
                            if let Some(track) = raw.into_summary() {
                                push_unique_track(&mut out.tracks, track);
                            }
                        }
                    } else if payload.get("title").is_some() {
                        if let Ok(raw) = serde_json::from_value::<RawAlbum>(payload.clone()) {
                            if let Some(album) = raw.into_summary() {
                                push_unique_album(&mut out.albums, album);
                            }
                        }
                    } else if let Ok(raw) = serde_json::from_value::<RawArtist>(payload.clone()) {
                        if let Some(artist) = raw.into_summary() {
                            push_unique_artist(&mut out.artists, artist);
                        }
                    }
                }
            }
        }
    }

    if let Some(included) = doc.get("included") {
        let parsed = parse_jsonapi_results(&serde_json::json!({ "included": included }));
        for track in parsed.tracks {
            push_unique_track(&mut out.tracks, track);
        }
        for album in parsed.albums {
            push_unique_album(&mut out.albums, album);
        }
        for artist in parsed.artists {
            push_unique_artist(&mut out.artists, artist);
        }
    }

    out.queries.truncate(6);
    out.tracks.truncate(4);
    out.albums.truncate(4);
    out.artists.truncate(4);
    out
}

fn collect_suggestion_queries(value: Option<&Value>, out: &mut Vec<String>) {
    let Some(entries) = value.and_then(Value::as_array) else {
        return;
    };
    for entry in entries {
        let raw = entry
            .as_str()
            .or_else(|| entry.get("query").and_then(Value::as_str))
            .unwrap_or_default()
            .trim();
        if !raw.is_empty() && !out.iter().any(|q| q.eq_ignore_ascii_case(raw)) {
            out.push(raw.to_string());
        }
    }
}

fn merge_suggestions(target: &mut SearchSuggestions, incoming: SearchSuggestions) {
    for query in incoming.queries {
        if target.queries.len() < 6
            && !target.queries.iter().any(|q| q.eq_ignore_ascii_case(&query))
        {
            target.queries.push(query);
        }
    }
    for track in incoming.tracks {
        if target.tracks.len() < 4 {
            push_unique_track(&mut target.tracks, track);
        }
    }
    for album in incoming.albums {
        if target.albums.len() < 4 {
            push_unique_album(&mut target.albums, album);
        }
    }
    for artist in incoming.artists {
        if target.artists.len() < 4 {
            push_unique_artist(&mut target.artists, artist);
        }
    }
}

fn push_unique_track(out: &mut Vec<TrackSummary>, track: TrackSummary) {
    if !out.iter().any(|existing| existing.id == track.id) {
        out.push(track);
    }
}

fn push_unique_album(out: &mut Vec<AlbumSummary>, album: AlbumSummary) {
    if !out.iter().any(|existing| existing.id == album.id) {
        out.push(album);
    }
}

fn push_unique_artist(out: &mut Vec<ArtistSummary>, artist: ArtistSummary) {
    if !out.iter().any(|existing| existing.id == artist.id) {
        out.push(artist);
    }
}

fn push_mix_from_results(
    out: &mut Vec<MixSummary>,
    seen: &mut std::collections::HashSet<String>,
    id: &str,
    title: &str,
    subtitle: &str,
    res: SearchResults,
) {
    if res.tracks.is_empty() || !seen.insert(title.to_string()) {
        return;
    }
    let image_id = res
        .albums
        .iter()
        .find_map(|album| album.cover_id.clone())
        .or_else(|| res.playlists.iter().find_map(|p| p.image_id.clone()))
        .or_else(|| res.tracks.iter().find_map(|t| t.cover_id.clone()));
    out.push(MixSummary {
        id: id.to_string(),
        title: title.to_string(),
        subtitle: subtitle.to_string(),
        image_id,
        tracks: res.tracks,
    });
}

/// Extrait les cartes de mix d'une page `v1 /pages/my_collection_my_mixes`.
fn parse_my_mixes_page(root: &Value) -> Vec<MixSummary> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    walk_mix_nodes(root, &mut out, &mut seen);
    out
}

fn walk_mix_nodes(
    val: &Value,
    out: &mut Vec<MixSummary>,
    seen: &mut std::collections::HashSet<String>,
) {
    match val {
        Value::Object(map) => {
            let title = map
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let id = match map.get("id") {
                Some(Value::String(s)) => s.trim().to_string(),
                Some(Value::Number(n)) => n.to_string(),
                _ => String::new(),
            };
            let has_mix_hint = map.contains_key("mixType")
                || map.contains_key("subTitle")
                || map.contains_key("subtitle");
            let is_mix_card = !title.is_empty()
                && !id.is_empty()
                && has_mix_hint
                && map.get("duration").is_none()
                && map.get("numberOfTracks").is_none();
            if is_mix_card {
                if seen.insert(id.clone()) {
                    let subtitle = map
                        .get("subTitle")
                        .or_else(|| map.get("subtitle"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    out.push(MixSummary {
                        id,
                        title,
                        subtitle,
                        image_id: extract_mix_image(map),
                        tracks: Vec::new(),
                    });
                }
                return;
            }
            for child in map.values() {
                walk_mix_nodes(child, out, seen);
            }
        }
        Value::Array(items) => {
            for item in items {
                walk_mix_nodes(item, out, seen);
            }
        }
        _ => {}
    }
}

fn extract_mix_image(map: &serde_json::Map<String, Value>) -> Option<String> {
    for key in ["images", "image", "squareImage", "cover", "artwork"] {
        if let Some(value) = map.get(key) {
            if let Some(url) = find_image_url(value) {
                return Some(url);
            }
        }
    }
    None
}

fn find_image_url(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Object(map) => {
            for key in ["LARGE", "MEDIUM", "SMALL", "url", "href"] {
                if let Some(url) = map.get(key).and_then(find_image_url) {
                    return Some(url);
                }
            }
            map.values().find_map(find_image_url)
        }
        _ => None,
    }
}

/// Helper générique `drain_pages` paginant par blocs de `page_size` avec
/// parallélisme borné par `max_concurrency` (`tokio::sync::Semaphore`).
pub async fn drain_pages<T, F, Fut>(
    page_size: usize,
    max_items: usize,
    max_concurrency: usize,
    fetch_page: F,
) -> Result<Vec<T>>
where
    T: DeserializeOwned + Send + 'static,
    F: Fn(usize, usize) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<PagedResponse<T>>> + Send + 'static,
{
    let page_size = page_size.max(1);
    let max_items = max_items.max(1);
    let first_limit = page_size.min(max_items);

    let first = fetch_page(0, first_limit).await?;
    let target_total = first.total_number_of_items.min(max_items);
    let mut results = first.items;

    if results.len() >= target_total || results.is_empty() {
        results.truncate(max_items);
        return Ok(results);
    }

    let mut offsets = Vec::new();
    let mut next_offset = results.len();
    while next_offset < target_total {
        let lim = page_size.min(target_total - next_offset);
        offsets.push((next_offset, lim));
        next_offset += lim;
    }

    let sem = Arc::new(Semaphore::new(max_concurrency.max(1)));
    let fetch_arc = Arc::new(fetch_page);
    let mut handles = Vec::with_capacity(offsets.len());

    for (idx, (off, lim)) in offsets.into_iter().enumerate() {
        let sem = Arc::clone(&sem);
        let fetch_arc = Arc::clone(&fetch_arc);
        handles.push(tokio::spawn(async move {
            let _permit = sem
                .acquire_owned()
                .await
                .map_err(|e| ApiError::Pagination(e.to_string()))?;
            let page = fetch_arc(off, lim).await?;
            Ok::<(usize, Vec<T>), ApiError>((idx, page.items))
        }));
    }

    let mut ordered: Vec<(usize, Vec<T>)> = Vec::with_capacity(handles.len());
    for handle in handles {
        let chunk = handle
            .await
            .map_err(|e| ApiError::Pagination(e.to_string()))??;
        ordered.push(chunk);
    }
    ordered.sort_by_key(|(idx, _)| *idx);
    for (_, mut chunk) in ordered {
        results.append(&mut chunk);
    }
    results.truncate(max_items);
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_tidal_track_wrapper_and_detects_hires_tag() {
        let payload = json!({
            "created": "2026-10-01T12:00:00.000+0000",
            "item": {
                "id": 251380837,
                "title": "Giorgio by Moroder",
                "duration": 544,
                "explicit": false,
                "audioQuality": "LOSSLESS",
                "mediaMetadata": { "tags": ["LOSSLESS", "HIRES_LOSSLESS"] },
                "artists": [{ "id": 1, "name": "Daft Punk" }],
                "album": {
                    "id": 10,
                    "title": "Random Access Memories",
                    "cover": "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
                }
            }
        });
        let wrapped: TrackItemWrapper = serde_json::from_value(payload).unwrap();
        let track = wrapped.into_track().unwrap();
        assert_eq!(track.id, 251380837);
        assert_eq!(track.artist_name, "Daft Punk");
        assert!(track.is_hires());
        assert_eq!(track.duration_label(), "9:04");
    }

    #[test]
    fn extracts_tracks_from_dynamic_hires_page_json() {
        let page = json!({
            "rows": [{
                "modules": [{
                    "type": "TRACK_LIST",
                    "pagedList": {
                        "items": [
                            {
                                "id": 42,
                                "title": "Take Five",
                                "duration": 324,
                                "audioQuality": "HI_RES_LOSSLESS",
                                "artist": { "id": 7, "name": "The Dave Brubeck Quartet" },
                                "album": { "id": 9, "title": "Time Out", "cover": "1111-2222" }
                            }
                        ]
                    }
                }]
            }]
        });
        let tracks = extract_tracks_from_page_json(&page, 20);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "Take Five");
        assert!(tracks[0].is_hires());
    }

    #[tokio::test]
    async fn drain_pages_preserves_order_across_concurrent_fetches() {
        let items: Vec<u32> = drain_pages(3, 10, 2, |offset, limit| async move {
            let end = (offset + limit).min(8);
            let slice: Vec<u32> = (offset as u32..end as u32).collect();
            Ok(PagedResponse {
                limit,
                offset,
                total_number_of_items: 8,
                items: slice,
            })
        })
        .await
        .unwrap();

        assert_eq!(items, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn parses_official_openapi_v2_jsonapi_document_with_artworks() {
        let doc = json!({
            "data": { "id": "me", "type": "userDiscoveryMixes" },
            "included": [
                {
                    "id": "art-1",
                    "type": "artworks",
                    "attributes": {
                        "mediaType": "IMAGE",
                        "files": [
                            { "href": "https://resources.tidal.com/images/aa/bb/320x320.jpg", "meta": { "width": 320, "height": 320 } },
                            { "href": "https://resources.tidal.com/images/aa/bb/640x640.jpg", "meta": { "width": 640, "height": 640 } }
                        ]
                    }
                },
                {
                    "id": "77",
                    "type": "artists",
                    "attributes": { "name": "Daft Punk", "popularity": 95 }
                },
                {
                    "id": "101",
                    "type": "albums",
                    "attributes": {
                        "title": "Discovery",
                        "numberOfItems": 14,
                        "mediaTags": ["LOSSLESS", "HIRES_LOSSLESS"]
                    },
                    "relationships": {
                        "artists": { "data": [{ "id": "77", "type": "artists" }] },
                        "coverArt": { "data": [{ "id": "art-1", "type": "artworks" }] }
                    }
                },
                {
                    "id": "555",
                    "type": "tracks",
                    "attributes": {
                        "title": "Digital Love",
                        "duration": "PT4M58S",
                        "explicit": false,
                        "mediaTags": ["LOSSLESS", "HIRES_LOSSLESS"]
                    },
                    "relationships": {
                        "artists": { "data": [{ "id": "77", "type": "artists" }] },
                        "albums": { "data": [{ "id": "101", "type": "albums" }] }
                    }
                }
            ]
        });
        let res = parse_jsonapi_results(&doc);
        assert_eq!(res.albums.len(), 1);
        assert_eq!(res.albums[0].title, "Discovery");
        assert_eq!(
            res.albums[0].cover_id.as_deref(),
            Some("https://resources.tidal.com/images/aa/bb/320x320.jpg")
        );
        assert_eq!(res.tracks.len(), 1);
        assert_eq!(res.tracks[0].title, "Digital Love");
        assert_eq!(res.tracks[0].duration_secs, 298);
        assert!(res.tracks[0].is_hires());
        assert_eq!(res.tracks[0].artist_name, "Daft Punk");
    }

    #[test]
    fn parses_favorite_album_ids_from_v1_and_openapi_shapes() {
        let v1 = json!({
            "TRACK": [1, 2, 3],
            "ALBUM": [10, "11", "not-a-number", 12]
        });
        assert_eq!(
            parse_v1_favorite_album_ids(&v1),
            Some(vec![10, 11, 12])
        );
        assert_eq!(
            parse_v1_favorite_album_ids(&json!({ "TRACK": [1] })),
            None,
            "l'absence de clé ALBUM doit déclencher le repli OpenAPI"
        );
        assert_eq!(
            parse_v1_favorite_album_ids(&json!({ "ALBUM": [] })),
            Some(Vec::new())
        );

        let openapi = json!({
            "data": [
                { "id": "42", "type": "albums" },
                { "id": "invalid", "type": "albums" },
                { "type": "albums" }
            ]
        });
        assert_eq!(parse_openapi_collection_album_ids(&openapi), Some(vec![42]));
        assert_eq!(
            parse_openapi_collection_album_ids(&json!({ "included": [] })),
            None
        );
    }

    #[test]
    fn parses_created_playlist_from_v1_response() {
        let body = json!({
            "uuid": "8f4d3e2a-1111-2222-3333-444455556666",
            "title": "Road Trip",
            "description": "Pour la route",
            "numberOfTracks": 0,
            "creator": { "id": 7, "name": "Kedi" },
            "squareImage": "abc-def"
        })
        .to_string();

        let playlist = parse_created_playlist_response(&body, "Fallback", "desc").unwrap();
        assert_eq!(playlist.uuid, "8f4d3e2a-1111-2222-3333-444455556666");
        assert_eq!(playlist.title, "Road Trip");
        assert_eq!(playlist.creator, "Kedi");
        assert_eq!(playlist.description.as_deref(), Some("Pour la route"));
        assert_eq!(playlist.image_id.as_deref(), Some("abc-def"));
        assert_eq!(playlist.number_of_tracks, 0);
    }

    #[test]
    fn parses_created_playlist_from_uuid_only_response() {
        let body = json!({ "uuid": "uuid-only" }).to_string();
        let playlist = parse_created_playlist_response(&body, "Nouvelle", "").unwrap();
        assert_eq!(playlist.uuid, "uuid-only");
        assert_eq!(playlist.title, "Nouvelle");
        assert_eq!(playlist.creator, "Moi");
        assert!(playlist.description.is_none());
    }

    #[test]
    fn rejects_created_playlist_response_without_uuid() {
        assert!(parse_created_playlist_response("{}", "T", "D").is_none());
        assert!(parse_created_playlist_response("pas du json", "T", "D").is_none());
        assert!(
            parse_created_playlist_response(&json!({ "uuid": "  " }).to_string(), "T", "D")
                .is_none()
        );
        assert!(
            parse_created_playlist_response(&json!({ "title": "Sans uuid" }).to_string(), "T", "D")
                .is_none()
        );
    }

    #[test]
    fn parses_created_playlist_from_jsonapi_response() {
        let body = json!({
            "data": {
                "id": "jsonapi-uuid",
                "type": "playlists",
                "attributes": {
                    "name": "Nom JSON:API",
                    "description": "Description JSON:API"
                }
            }
        })
        .to_string();
        let playlist = parse_jsonapi_created_playlist(&body, "Fallback", "desc").unwrap();
        assert_eq!(playlist.uuid, "jsonapi-uuid");
        assert_eq!(playlist.title, "Nom JSON:API");
        assert_eq!(playlist.description.as_deref(), Some("Description JSON:API"));

        assert!(parse_jsonapi_created_playlist(&json!({ "data": {} }).to_string(), "T", "D").is_none());
        assert!(parse_jsonapi_created_playlist("nope", "T", "D").is_none());
    }

    #[test]
    fn parses_user_playlist_listing_wrapper_shapes() {
        use crate::models::PlaylistItemWrapper;

        let nested = json!({
            "playlist": {
                "uuid": "pl-uuid",
                "title": "Mes Mixes",
                "numberOfTracks": 12,
                "squareImage": "img-uuid",
                "creator": { "name": "Kedi" }
            }
        });
        let direct = json!({
            "uuid": "pl-direct",
            "title": "Playlist directe",
            "numberOfTracks": 3
        });

        let nested = serde_json::from_value::<PlaylistItemWrapper>(nested)
            .unwrap()
            .into_playlist()
            .unwrap();
        assert_eq!(nested.uuid, "pl-uuid");
        assert_eq!(nested.title, "Mes Mixes");
        assert_eq!(nested.number_of_tracks, 12);
        assert_eq!(nested.image_id.as_deref(), Some("img-uuid"));
        assert_eq!(nested.creator, "Kedi");

        let direct = serde_json::from_value::<PlaylistItemWrapper>(direct)
            .unwrap()
            .into_playlist()
            .unwrap();
        assert_eq!(direct.uuid, "pl-direct");
        assert_eq!(direct.title, "Playlist directe");
        assert_eq!(direct.creator, "TIDAL");
    }

    #[test]
    fn clean_tidal_bio_strips_wimp_links_and_html() {
        let raw = "[wimpLink artistId=\"27\"]Daft Punk[/wimpLink] est un duo<br/>français&nbsp;formé en 1993.";
        assert_eq!(
            clean_tidal_bio(raw),
            "Daft Punk est un duo français formé en 1993."
        );

        let nested = "<i>Discovery</i> reste leur album le plus [wimpLink artistId=\"42\"]pop[/wimpLink].";
        assert_eq!(
            clean_tidal_bio(nested),
            "Discovery reste leur album le plus pop."
        );

        assert_eq!(clean_tidal_bio("   "), "");
        assert_eq!(clean_tidal_bio("Note [1] conservée."), "Note [1] conservée.");
        assert_eq!(clean_tidal_bio("Déjà propre &amp; net"), "Déjà propre & net");
    }

    #[test]
    fn track_wrapper_parses_artist_id_from_artists_and_artist() {
        let with_artists: TrackItemWrapper = serde_json::from_value(json!({
            "item": {
                "id": 42,
                "title": "Take Five",
                "duration": 324,
                "artists": [{ "id": 7, "name": "The Dave Brubeck Quartet" }],
                "album": { "id": 9, "title": "Time Out" }
            }
        }))
        .unwrap();
        let track = with_artists.into_track().unwrap();
        assert_eq!(track.artist_id, Some(7));

        let with_artist: TrackItemWrapper = serde_json::from_value(json!({
            "id": 43,
            "title": "Blue Rondo",
            "duration": 400,
            "artist": { "id": 8, "name": "Dave Brubeck" },
            "album": { "id": 9, "title": "Time Out" }
        }))
        .unwrap();
        assert_eq!(with_artist.into_track().unwrap().artist_id, Some(8));

        let without_artist: TrackItemWrapper = serde_json::from_value(json!({
            "id": 44,
            "title": "Sans artiste",
            "duration": 100
        }))
        .unwrap();
        assert_eq!(without_artist.into_track().unwrap().artist_id, None);
    }

    #[test]
    fn parse_jsonapi_results_populates_artist_ids_everywhere() {
        let doc = json!({
            "included": [
                { "id": "77", "type": "artists", "attributes": { "name": "Daft Punk" } },
                {
                    "id": "101",
                    "type": "albums",
                    "attributes": { "title": "Discovery" },
                    "relationships": {
                        "artists": { "data": [{ "id": "77", "type": "artists" }] }
                    }
                },
                {
                    "id": "555",
                    "type": "tracks",
                    "attributes": { "title": "Digital Love", "duration": "PT4M58S" },
                    "relationships": {
                        "artists": { "data": [{ "id": "77", "type": "artists" }] },
                        "albums": { "data": [{ "id": "101", "type": "albums" }] }
                    }
                }
            ]
        });
        let res = parse_jsonapi_results(&doc);
        assert_eq!(res.albums[0].artist_id, Some(77));
        assert_eq!(res.tracks[0].artist_id, Some(77));
        assert_eq!(res.tracks[0].album_id, Some(101));
        assert_eq!(res.artists[0].id, 77);
    }

    #[test]
    fn parses_search_suggestions_from_v1_shape() {
        let doc = json!({
            "suggestions": [
                { "query": "daft punk" },
                { "query": "Daft Punk" },
                "daft punk random access memories",
                { "query": "   " }
            ],
            "directHits": [
                {
                    "type": "TRACK",
                    "value": {
                        "id": 251380837,
                        "title": "Giorgio by Moroder",
                        "duration": 544,
                        "artists": [{ "id": 27, "name": "Daft Punk" }],
                        "album": { "id": 79481220, "title": "Random Access Memories", "cover": "abc" }
                    }
                },
                {
                    "type": "ALBUM",
                    "value": {
                        "id": 79481220,
                        "title": "Random Access Memories",
                        "artists": [{ "id": 27, "name": "Daft Punk" }],
                        "cover": "abc",
                        "numberOfTracks": 13
                    }
                },
                {
                    "type": "ARTIST",
                    "value": { "id": 27, "name": "Daft Punk", "picture": "pic-uuid" }
                }
            ]
        });
        let suggestions = parse_search_suggestions(&doc);
        assert_eq!(
            suggestions.queries,
            vec!["daft punk", "daft punk random access memories"]
        );
        assert_eq!(suggestions.tracks.len(), 1);
        assert_eq!(suggestions.tracks[0].artist_id, Some(27));
        assert_eq!(suggestions.albums[0].id, 79481220);
        assert_eq!(suggestions.albums[0].artist_id, Some(27));
        assert_eq!(suggestions.artists[0].id, 27);
        assert_eq!(
            suggestions.artists[0].picture_url(320).as_deref(),
            Some("https://resources.tidal.com/images/pic/uuid/320x320.jpg")
        );
    }

    #[test]
    fn parses_search_suggestions_from_openapi_v2_shape() {
        let doc = json!({
            "data": {
                "id": "suggestions-1",
                "type": "searchSuggestions",
                "attributes": {
                    "suggestions": [
                        { "query": "daft punk" },
                        { "query": "daft punk alive" }
                    ]
                }
            },
            "included": [
                { "id": "27", "type": "artists", "attributes": { "name": "Daft Punk" } },
                {
                    "id": "101",
                    "type": "albums",
                    "attributes": { "title": "Discovery", "numberOfItems": 14 },
                    "relationships": {
                        "artists": { "data": [{ "id": "27", "type": "artists" }] }
                    }
                },
                {
                    "id": "555",
                    "type": "tracks",
                    "attributes": { "title": "Digital Love", "duration": "PT5M1S" },
                    "relationships": {
                        "artists": { "data": [{ "id": "27", "type": "artists" }] },
                        "albums": { "data": [{ "id": "101", "type": "albums" }] }
                    }
                }
            ]
        });
        let suggestions = parse_search_suggestions(&doc);
        assert_eq!(
            suggestions.queries,
            vec!["daft punk", "daft punk alive"]
        );
        assert_eq!(suggestions.tracks[0].title, "Digital Love");
        assert_eq!(suggestions.tracks[0].artist_id, Some(27));
        assert_eq!(suggestions.tracks[0].duration_secs, 301);
        assert_eq!(suggestions.albums[0].id, 101);
        assert_eq!(suggestions.albums[0].artist_id, Some(27));
        assert_eq!(suggestions.artists[0].name, "Daft Punk");
    }

    #[test]
    fn parses_favorite_albums_v1_wrapped_and_direct_items() {
        let wrapped = json!({
            "limit": 50,
            "offset": 0,
            "totalNumberOfItems": 2,
            "items": [
                {
                    "created": "2026-01-01T00:00:00.000+0000",
                    "item": {
                        "id": 79481220,
                        "title": "Random Access Memories",
                        "cover": "cover-uuid",
                        "releaseDate": "2013-05-17",
                        "numberOfTracks": 13,
                        "artists": [{ "id": 27, "name": "Daft Punk" }]
                    }
                },
                {
                    "id": 79481220,
                    "title": "Doublon",
                    "artists": [{ "id": 27, "name": "Daft Punk" }]
                },
                {
                    "id": 145623,
                    "title": "Kind of Blue",
                    "artist": { "id": 56, "name": "Miles Davis" }
                }
            ]
        });
        let albums = parse_favorite_albums_v1(&wrapped);
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].id, 79481220);
        assert_eq!(albums[0].artist_id, Some(27));
        assert_eq!(albums[0].number_of_tracks, 13);
        assert_eq!(albums[1].artist_id, Some(56));

        let bare = json!([
            { "id": 42, "title": "Discovery", "artists": [{ "id": 27, "name": "Daft Punk" }] }
        ]);
        assert_eq!(parse_favorite_albums_v1(&bare)[0].artist_id, Some(27));
        assert!(parse_favorite_albums_v1(&json!({ "items": [] })).is_empty());
    }

    #[test]
    fn parses_favorite_artists_v1_wrapped_and_direct_items() {
        let wrapped = json!({
            "items": [
                {
                    "created": "2026-01-01T00:00:00.000+0000",
                    "item": { "id": 27, "name": "Daft Punk", "picture": "pic-uuid" }
                },
                { "id": 56, "name": "Miles Davis" },
                { "id": 27, "name": "Daft Punk" }
            ]
        });
        let artists = parse_favorite_artists_v1(&wrapped);
        assert_eq!(artists.len(), 2);
        assert_eq!(artists[0].id, 27);
        assert_eq!(artists[0].picture_id.as_deref(), Some("pic-uuid"));
        assert_eq!(
            artists[0].picture_url(640).as_deref(),
            Some("https://resources.tidal.com/images/pic/uuid/640x640.jpg")
        );
        assert_eq!(artists[1].name, "Miles Davis");

        assert!(parse_favorite_artists_v1(&json!({ "items": [] })).is_empty());
        assert!(parse_favorite_artists_v1(&json!("nope")).is_empty());
    }

    #[test]
    fn extracts_mix_tracks_from_v1_pages_mix_fixture() {
        let page = json!({
            "categories": [
                { "title": "My Mix", "type": "MIX_HEADER" },
                {
                    "title": "Titres",
                    "items": [
                        {
                            "id": 1,
                            "title": "One",
                            "duration": 180,
                            "artists": [{ "id": 7, "name": "Artist" }],
                            "album": { "id": 9, "title": "Album", "cover": "cover-1" }
                        },
                        {
                            "id": 2,
                            "title": "Two",
                            "duration": 240,
                            "artists": [{ "id": 8, "name": "Other" }],
                            "album": { "id": 10, "title": "Album 2", "cover": "cover-2" }
                        }
                    ]
                }
            ]
        });

        let tracks = extract_tracks_from_page_json(&page, 50);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].title, "One");
        assert_eq!(tracks[0].artist_id, Some(7));
        assert_eq!(tracks[0].album_id, Some(9));
        assert_eq!(tracks[1].duration_secs, 240);
        assert_eq!(extract_tracks_from_page_json(&page, 1).len(), 1);
    }

    #[test]
    fn parses_my_mixes_page_cards_with_images() {
        let page = json!({
            "rows": [{
                "modules": [{
                    "type": "MIX_LIST",
                    "pagedList": {
                        "items": [
                            {
                                "id": "mix-1",
                                "title": "My Mix 1",
                                "subTitle": "Daft Punk, Justice",
                                "mixType": "USER_MIX",
                                "images": {
                                    "SMALL": { "width": 160, "height": 160, "url": "https://resources.tidal.com/images/aa/bb/160x160.jpg" }
                                }
                            },
                            {
                                "id": 251380837,
                                "title": "Giorgio by Moroder",
                                "duration": 544,
                                "artists": [{ "id": 27, "name": "Daft Punk" }]
                            }
                        ]
                    }
                }]
            }]
        });
        let mixes = parse_my_mixes_page(&page);
        assert_eq!(mixes.len(), 1);
        assert_eq!(mixes[0].title, "My Mix 1");
        assert_eq!(mixes[0].subtitle, "Daft Punk, Justice");
        assert_eq!(
            mixes[0].image_url(320).as_deref(),
            Some("https://resources.tidal.com/images/aa/bb/160x160.jpg")
        );
    }

    #[test]
    fn parses_track_lyrics_from_v1_payload_with_subtitles_and_provider() {
        let value = json!({
            "lyrics": "Hello\nWorld",
            "subtitles": "[00:12.34]Hello\n[00:15.000]World",
            "lyricsProvider": "LYRICFIND"
        });
        let parsed = parse_track_lyrics(42, &value).expect("paroles v1");
        assert_eq!(parsed.track_id, 42);
        assert_eq!(parsed.plain_text.as_deref(), Some("Hello\nWorld"));
        assert_eq!(parsed.provider.as_deref(), Some("LYRICFIND"));
        assert_eq!(parsed.synced_lines.len(), 2);
        assert_eq!(parsed.synced_lines[0].time_ms, 12_340);
        assert_eq!(parsed.synced_lines[1].time_ms, 15_000);
        assert_eq!(parsed.synced_lines[1].text, "World");
    }

    #[test]
    fn parses_track_lyrics_from_openapi_v2_included_resource() {
        let doc = json!({
            "data": {
                "id": "42",
                "type": "tracks",
                "relationships": {
                    "lyrics": { "data": [{ "id": "l1", "type": "lyrics" }] }
                }
            },
            "included": [{
                "id": "l1",
                "type": "lyrics",
                "attributes": {
                    "text": "Ligne unique",
                    "subtitles": "[01:02.345]Ligne unique",
                    "provider": "TIDAL"
                }
            }]
        });
        let parsed = parse_track_lyrics(42, &doc).expect("paroles OpenAPI v2");
        assert_eq!(parsed.plain_text.as_deref(), Some("Ligne unique"));
        assert_eq!(parsed.provider.as_deref(), Some("TIDAL"));
        assert_eq!(parsed.synced_lines.len(), 1);
        assert_eq!(parsed.synced_lines[0].time_ms, 62_345);
    }

    #[test]
    fn track_lyrics_parser_derives_plain_text_and_handles_empty_payloads() {
        let only_synced = json!({ "subtitles": "[00:01.00]A\n[00:02.00]B" });
        let parsed = parse_track_lyrics(7, &only_synced).expect("paroles synchronisées");
        assert_eq!(parsed.plain_text.as_deref(), Some("A\nB"));
        assert!(parsed.provider.is_none());

        assert!(parse_track_lyrics(42, &json!({})).is_none());
        assert!(parse_track_lyrics(42, &json!({ "lyrics": "", "subtitles": "" })).is_none());
        assert!(parse_track_lyrics(42, &json!({ "included": [] })).is_none());

        // Certaines réponses v2 renvoient directement le LRC dans `text`.
        let lrc_in_text = json!({ "text": "[00:05.00]Première\n[00:09.50]Deuxième" });
        let parsed = parse_track_lyrics(3, &lrc_in_text).expect("LRC dans text");
        assert_eq!(parsed.synced_lines.len(), 2);
        assert_eq!(parsed.synced_lines[0].time_ms, 5_000);
        assert_eq!(parsed.synced_lines[1].time_ms, 9_500);
    }

    #[test]
    fn parses_track_credits_and_deduplicates_roles_and_contributors() {
        let value = json!([
            {
                "type": "Producer",
                "contributors": [
                    { "name": "Daft Punk" },
                    { "name": "  Daft Punk  " },
                    { "name": "" }
                ]
            },
            {
                "type": "Producer",
                "contributors": [{ "name": "Julian Casablancas" }]
            },
            {
                "type": "Mixer",
                "contributors": [{ "name": "Mick Guzauski" }]
            },
            { "type": "Empty", "contributors": [] },
            { "type": "", "contributors": [{ "name": "Ignoré" }] }
        ]);
        let credits = parse_track_credits(42, &value);
        assert_eq!(credits.track_id, 42);
        assert_eq!(credits.groups.len(), 2);
        assert_eq!(credits.groups[0].role, "Producer");
        assert_eq!(
            credits.groups[0].contributors,
            vec!["Daft Punk", "Julian Casablancas"]
        );
        assert_eq!(credits.groups[1].role, "Mixer");
        assert_eq!(credits.groups[1].contributors, vec!["Mick Guzauski"]);
    }

    #[test]
    fn parses_track_credits_items_wrapper_and_tolerates_missing_fields() {
        let wrapped = json!({
            "items": [
                {
                    "type": "Composer",
                    "contributors": [{ "name": "A" }, "B", { "name": "  " }]
                }
            ]
        });
        let credits = parse_track_credits(9, &wrapped);
        assert_eq!(credits.groups.len(), 1);
        assert_eq!(credits.groups[0].role, "Composer");
        assert_eq!(credits.groups[0].contributors, vec!["A", "B"]);

        assert!(parse_track_credits(1, &json!({})).groups.is_empty());
        assert!(parse_track_credits(1, &json!({ "items": [] })).groups.is_empty());
    }

    #[test]
    fn parses_track_radio_v1_items_with_wrapped_and_direct_tracks() {
        let page = json!({
            "limit": 40,
            "offset": 0,
            "totalNumberOfItems": 3,
            "items": [
                {
                    "item": {
                        "id": 1,
                        "title": "One",
                        "duration": 180,
                        "artists": [{ "id": 7, "name": "Artist" }],
                        "album": { "id": 9, "title": "Album", "cover": "cover-1" }
                    }
                },
                {
                    "id": 2,
                    "title": "Two",
                    "duration": 240,
                    "artist": { "id": 8, "name": "Other" }
                },
                {
                    "item": { "id": 1, "title": "Doublon", "duration": 180 }
                }
            ]
        });
        let tracks = parse_radio_tracks(&page);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].id, 1);
        assert_eq!(tracks[0].title, "One");
        assert_eq!(tracks[0].artist_id, Some(7));
        assert_eq!(tracks[0].album_id, Some(9));
        assert_eq!(tracks[1].id, 2);
        assert_eq!(tracks[1].artist_name, "Other");

        let bare = json!([
            { "id": 3, "title": "Trois", "duration": 120, "artists": [{ "id": 7, "name": "Artist" }] }
        ]);
        assert_eq!(parse_radio_tracks(&bare).len(), 1);
        assert!(parse_radio_tracks(&json!({})).is_empty());
        assert!(parse_radio_tracks(&json!({ "items": [] })).is_empty());
    }

    #[test]
    fn parses_track_radio_openapi_v2_similar_tracks_document() {
        let doc = json!({
            "data": [{ "id": "555", "type": "tracks" }],
            "included": [
                { "id": "27", "type": "artists", "attributes": { "name": "Daft Punk" } },
                {
                    "id": "101",
                    "type": "albums",
                    "attributes": { "title": "Discovery" },
                    "relationships": {
                        "artists": { "data": [{ "id": "27", "type": "artists" }] }
                    }
                },
                {
                    "id": "555",
                    "type": "tracks",
                    "attributes": { "title": "Digital Love", "duration": "PT4M58S" },
                    "relationships": {
                        "artists": { "data": [{ "id": "27", "type": "artists" }] },
                        "albums": { "data": [{ "id": "101", "type": "albums" }] }
                    }
                }
            ]
        });
        let tracks = parse_jsonapi_results(&doc).tracks;
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, 555);
        assert_eq!(tracks[0].title, "Digital Love");
        assert_eq!(tracks[0].artist_id, Some(27));
        assert_eq!(tracks[0].album_id, Some(101));
        assert_eq!(tracks[0].duration_secs, 298);
    }
}
