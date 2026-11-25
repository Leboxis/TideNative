//! Modèles partagés, formats PCM, résolution centralisée des chemins système
//! et protocole de commande/événements du lecteur `TideNative`.

pub mod paths;

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

/// Palier de qualité audio demandé à l'API TIDAL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum AudioQuality {
    /// AAC 96 kbps
    Low,
    /// AAC 320 kbps
    High,
    /// FLAC 16-bit / 44.1 kHz
    Lossless,
    /// FLAC jusqu'à 24-bit / 192 kHz (MPEG-DASH Hi-Res)
    #[default]
    HiResLossless,
}

impl AudioQuality {
    pub const fn as_api_str(self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::High => "HIGH",
            Self::Lossless => "LOSSLESS",
            Self::HiResLossless => "HI_RES_LOSSLESS",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Low => "AAC 96 kbps",
            Self::High => "AAC 320 kbps",
            Self::Lossless => "FLAC 16-bit / 44.1 kHz",
            Self::HiResLossless => "Hi-Res FLAC 24-bit / 192 kHz",
        }
    }
}

impl fmt::Display for AudioQuality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_api_str())
    }
}

/// Format d'un flux PCM décodé (échantillons internes normalisés en `i32` alignés
/// MSB ou valeur signée `bits_per_sample`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PcmFormat {
    /// Fréquence d'échantillonnage en Hz (`44100`, `48000`, `88200`, `96000`, `176400`, `192000`).
    pub sample_rate: u32,
    /// Nombre de canaux (`2` pour la stéréo).
    pub channels: u16,
    /// Profondeur de bits réelle du flux décodé (`16`, `24` ou `32`).
    pub bits_per_sample: u16,
}

impl PcmFormat {
    pub const fn stereo(sample_rate: u32, bits_per_sample: u16) -> Self {
        Self {
            sample_rate,
            channels: 2,
            bits_per_sample,
        }
    }

    /// Nombre d'échantillons scalaires (`i32`) par trame multicanal.
    pub const fn samples_per_frame(self) -> usize {
        self.channels as usize
    }

    /// Nombre de trames correspondant à une durée donnée.
    pub fn frames_for_duration(self, duration: Duration) -> usize {
        let secs = duration.as_secs_f64();
        ((secs * self.sample_rate as f64).round() as usize).max(1)
    }

    /// Durée en millisecondes correspondant à un nombre de trames PCM.
    pub fn frames_to_ms(self, frames: u64) -> u64 {
        if self.sample_rate == 0 {
            return 0;
        }
        (frames * 1000) / u64::from(self.sample_rate)
    }

    /// Format court type `"24-bit / 192.0 kHz"`.
    pub fn short_label(self) -> String {
        let khz = f64::from(self.sample_rate) / 1000.0;
        if self.sample_rate % 1000 == 0 {
            format!("{}-bit / {:.0} kHz", self.bits_per_sample, khz)
        } else {
            format!("{}-bit / {:.1} kHz", self.bits_per_sample, khz)
        }
    }
}

/// Métadonnées complètes du pipeline audio actif (affichées dans la barre de lecture).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct StreamFormatInfo {
    /// Codec source (`"FLAC"`, `"AAC"`, etc.).
    pub codec: String,
    /// Format PCM issu du décodeur.
    pub source_pcm: PcmFormat,
    /// Profondeur de bits du conteneur matériel WASAPI négocié avec le DAC (`16`, `24` ou `32`).
    pub dac_container_bits: u16,
    /// Bits valides acceptés par le DAC (`16`, `24` ou `32`).
    pub dac_valid_bits: u16,
    /// `true` si `IAudioClient` tourne en `AUDCLNT_SHAREMODE_EXCLUSIVE`.
    pub is_exclusive: bool,
    /// `true` si aucun rééchantillonnage ni atténuation logicielle n'est appliqué.
    pub is_bit_perfect: bool,
    /// Nom du périphérique WASAPI actif.
    pub device_name: String,
}

impl StreamFormatInfo {
    /// Badge audiophile complet pour l'UI :
    /// ex. `"FLAC • 24-bit / 192 kHz • WASAPI Exclusif • Bit-Perfect"`.
    pub fn audiophile_badge(&self) -> String {
        let mode = if self.is_exclusive {
            "WASAPI Exclusif"
        } else {
            "WASAPI Partagé"
        };
        let bp = if self.is_bit_perfect {
            " • Bit-Perfect"
        } else {
            ""
        };
        format!(
            "{} • {} • {}{}",
            self.codec,
            self.source_pcm.short_label(),
            mode,
            bp
        )
    }
}

/// Périphérique de sortie audio Windows Core Audio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AudioDeviceDescriptor {
    /// Identifiant d'endpoint WASAPI (`IMMDevice::GetId`).
    pub id: String,
    /// Nom lisible du DAC / carte son (`PKEY_Device_FriendlyName`).
    pub name: String,
    /// `true` s'il s'agit du périphérique multimédia par défaut de Windows.
    pub is_default: bool,
}

/// Résumé d'un titre du catalogue TIDAL (optimisé pour le cache SQLite et les listes virtualisées).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrackSummary {
    pub id: u64,
    pub title: String,
    pub artist_name: String,
    /// Identifiant TIDAL de l'artiste principal (facultatif, absent des anciens snapshots).
    #[serde(default)]
    pub artist_id: Option<u64>,
    #[serde(default)]
    pub album_id: Option<u64>,
    pub album_title: String,
    /// UUID de pochette TIDAL (`xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`).
    pub cover_id: Option<String>,
    pub duration_secs: u32,
    pub audio_quality: Option<String>,
    pub explicit: bool,
    #[serde(default)]
    pub dolby_audio: bool,
}

impl TrackSummary {
    /// Construit l'URL CDN TIDAL de la pochette (`160x160`, `320x320`, `640x640`, `1280x1280`).
    pub fn cover_url(&self, size: u16) -> Option<String> {
        tidal_image_url(self.cover_id.as_deref()?, size)
    }

    /// Indique si le titre dispose d'un master Hi-Res FLAC (24-bit).
    pub fn is_hires(&self) -> bool {
        self.audio_quality
            .as_deref()
            .is_some_and(|q| q.eq_ignore_ascii_case("HI_RES_LOSSLESS") || q.eq_ignore_ascii_case("HIRES_LOSSLESS"))
    }

    /// Durée formatée `"mm:ss"`.
    pub fn duration_label(&self) -> String {
        format_secs(self.duration_secs)
    }
}

/// Une ligne de paroles synchronisées au format LRC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LyricsLine {
    /// Horodatage de la ligne, en millisecondes depuis le début du titre.
    pub time_ms: u64,
    pub text: String,
}

/// Paroles d'un titre TIDAL : texte brut et/ou lignes synchronisées LRC.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrackLyrics {
    pub track_id: u64,
    pub plain_text: Option<String>,
    pub synced_lines: Vec<LyricsLine>,
    pub provider: Option<String>,
}

/// Parse des paroles synchronisées au format LRC :
/// `[00:12.34]Hello`, `[01:02.345]Line`, `[02:05]Line`, timestamps multiples
/// `[00:10.00][00:45.50]Refrain`. Les balises de métadonnées (`[ar:...]`,
/// `[ti:...]`, `[al:...]`, `[by:...]`, `[offset:...]`) sont ignorées, les lignes
/// au texte vide sont sautées et le résultat est trié par `time_ms` croissant.
/// Un BOM UTF-8, les retours chariot Windows et les espaces entre plusieurs
/// horodatages sont tolérés ; les horodatages négatifs ou malformés sont ignorés.
pub fn parse_synced_lyrics(subtitles: &str) -> Vec<LyricsLine> {
    let mut lines = Vec::new();
    for raw_line in subtitles.lines() {
        let raw_line = raw_line.strip_prefix('\u{feff}').unwrap_or(raw_line);
        let mut rest = raw_line.trim_start();
        let mut timestamps = Vec::new();
        loop {
            let trimmed = rest.trim_start();
            let Some(tag_body) = trimmed.strip_prefix('[') else {
                rest = trimmed;
                break;
            };
            let Some(close) = tag_body.find(']') else {
                // Crochet non fermé : la ligne entière est traitée comme du texte
                // non horodaté et sera ignorée faute d'horodatage.
                rest = trimmed;
                break;
            };
            if let Some(time_ms) = parse_lrc_timestamp(&tag_body[..close]) {
                timestamps.push(time_ms);
            }
            rest = &tag_body[close + 1..];
        }
        let text = rest.trim();
        if timestamps.is_empty() || text.is_empty() {
            continue;
        }
        for time_ms in timestamps {
            lines.push(LyricsLine {
                time_ms,
                text: text.to_string(),
            });
        }
    }
    lines.sort_by_key(|line| line.time_ms);
    lines
}

/// Parse une balise temporelle LRC `mm:ss`, `mm:ss.xx` ou `mm:ss.xxx` en millisecondes.
fn parse_lrc_timestamp(tag: &str) -> Option<u64> {
    let (minutes, remainder) = tag.split_once(':')?;
    let minutes: u64 = minutes.trim().parse().ok()?;
    let (seconds, fraction) = match remainder.split_once('.') {
        Some((secs, frac)) => (secs, Some(frac)),
        None => (remainder, None),
    };
    let seconds: u64 = seconds.trim().parse().ok()?;
    let fraction_ms = match fraction {
        Some(frac) => {
            let digits: String = frac.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                0
            } else {
                let value: u64 = digits.parse().ok()?;
                let len = digits.len() as u32;
                if len <= 3 {
                    value * 10u64.pow(3 - len)
                } else {
                    value / 10u64.pow(len - 3)
                }
            }
        }
        None => 0,
    };
    Some(
        minutes
            .saturating_mul(60_000)
            .saturating_add(seconds.saturating_mul(1_000))
            .saturating_add(fraction_ms),
    )
}

/// Index de la dernière ligne dont `time_ms <= position_ms`
/// (via `partition_point`), ou `None` si la liste est vide ou avant la première ligne.
pub fn active_lyrics_index(lines: &[LyricsLine], position_ms: u64) -> Option<usize> {
    let idx = lines.partition_point(|line| line.time_ms <= position_ms);
    idx.checked_sub(1)
}

/// Groupe de contributeurs d'un même rôle (production, mixage, écriture...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrackCreditGroup {
    pub role: String,
    pub contributors: Vec<String>,
}

/// Crédits complets d'un titre TIDAL, regroupés par rôle.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrackCredits {
    pub track_id: u64,
    pub groups: Vec<TrackCreditGroup>,
}

/// Construit une URL d'image CDN TIDAL à partir d'un UUID (`xxxx-xxxx-...`)
/// ou renvoie directement l'URL si une URL complète OpenAPI v2 (`https://...`) est fournie.
pub fn tidal_image_url(uuid_or_url: &str, size: u16) -> Option<String> {
    let trimmed = uuid_or_url.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        return Some(trimmed.to_string());
    }
    let path = trimmed.replace('-', "/");
    Some(format!(
        "https://resources.tidal.com/images/{path}/{size}x{size}.jpg"
    ))
}

/// Formate un nombre de secondes en `"m:ss"` ou `"h:mm:ss"`.
pub fn format_secs(total_secs: u32) -> String {
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Résumé d'une playlist TIDAL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlaylistSummary {
    pub uuid: String,
    pub title: String,
    pub description: Option<String>,
    pub creator: String,
    pub number_of_tracks: u32,
    pub image_id: Option<String>,
}

impl PlaylistSummary {
    pub fn image_url(&self, size: u16) -> Option<String> {
        tidal_image_url(self.image_id.as_deref()?, size)
    }
}

/// Dossier de playlists TIDAL regroupant plusieurs playlists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlaylistFolder {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub total_number_of_items: u32,
    #[serde(default)]
    pub playlists: Vec<PlaylistSummary>,
}

/// Résumé d'un album TIDAL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AlbumSummary {
    pub id: u64,
    pub title: String,
    pub artist_name: String,
    /// Identifiant TIDAL de l'artiste principal (facultatif, absent des anciens snapshots).
    #[serde(default)]
    pub artist_id: Option<u64>,
    pub cover_id: Option<String>,
    pub release_date: Option<String>,
    pub audio_quality: Option<String>,
    pub number_of_tracks: u32,
}

impl AlbumSummary {
    pub fn cover_url(&self, size: u16) -> Option<String> {
        tidal_image_url(self.cover_id.as_deref()?, size)
    }
}

/// Résumé d'un artiste TIDAL.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ArtistSummary {
    pub id: u64,
    pub name: String,
    pub picture_id: Option<String>,
}

impl ArtistSummary {
    /// Construit l'URL CDN TIDAL de la photo d'artiste (`160x160`, `320x320`, `640x640`...).
    pub fn picture_url(&self, size: u16) -> Option<String> {
        tidal_image_url(self.picture_id.as_deref()?, size)
    }
}

/// Fiche complète d'un artiste (profil, biographie, discographie et artistes similaires),
/// assemblée depuis les endpoints TIDAL v1 (`artists/{id}`, `/bio`, `/toptracks`, `/albums`, `/similar`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ArtistDetailData {
    pub artist: ArtistSummary,
    pub bio: Option<String>,
    pub top_tracks: Vec<TrackSummary>,
    pub albums: Vec<AlbumSummary>,
    pub eps_singles: Vec<AlbumSummary>,
    pub compilations: Vec<AlbumSummary>,
    pub similar_artists: Vec<ArtistSummary>,
}

/// Suggestions instantanées d'autocomplétion (requêtes + résultats directs).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SearchSuggestions {
    pub queries: Vec<String>,
    pub tracks: Vec<TrackSummary>,
    pub albums: Vec<AlbumSummary>,
    pub artists: Vec<ArtistSummary>,
}

/// Carte de mix personnalisé TIDAL (Daily Mix, Découverte, Nouveautés...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MixSummary {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub image_id: Option<String>,
    pub tracks: Vec<TrackSummary>,
}

impl MixSummary {
    pub fn image_url(&self, size: u16) -> Option<String> {
        tidal_image_url(self.image_id.as_deref()?, size)
    }
}

/// Ordre de tri des listes de titres dans l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum TrackSortOrder {
    #[default]
    Default,
    TitleAsc,
    ArtistAsc,
    AlbumAsc,
    DurationDesc,
}

impl TrackSortOrder {
    pub const ALL: [Self; 5] = [
        Self::Default,
        Self::TitleAsc,
        Self::ArtistAsc,
        Self::AlbumAsc,
        Self::DurationDesc,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "Ordre d'ajout",
            Self::TitleAsc => "Titre (A–Z)",
            Self::ArtistAsc => "Artiste (A–Z)",
            Self::AlbumAsc => "Album (A–Z)",
            Self::DurationDesc => "Durée",
        }
    }
}

/// Meilleur résultat mis en avant lors d'une recherche catalogue (`topHit` v1 / `topHits` OpenAPI v2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", content = "value")]
pub enum TopHit {
    Track(TrackSummary),
    Album(AlbumSummary),
    Artist(ArtistSummary),
    Playlist(PlaylistSummary),
}

/// Résultats consolidés d'une recherche catalogue TIDAL.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SearchResults {
    pub tracks: Vec<TrackSummary>,
    pub albums: Vec<AlbumSummary>,
    pub playlists: Vec<PlaylistSummary>,
    pub artists: Vec<ArtistSummary>,
    #[serde(default)]
    pub top_hit: Option<TopHit>,
}

/// Données dynamiques de l'onglet **Explorer** chargées depuis les endpoints TIDAL officiels
/// (OpenAPI v2 `userNewReleaseMixes`, `userDiscoveryMixes`, `userDailyMixes`, `searchResults` + pages catalogue).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ExploreHubData {
    pub spotlight_albums: Vec<AlbumSummary>,
    pub featured_playlists: Vec<PlaylistSummary>,
    pub new_release_tracks: Vec<TrackSummary>,
    pub discovery_tracks: Vec<TrackSummary>,
    /// Miniatures réelles indexées par nom de catégorie ou genre (`"Nouveautés"`, `"Hip-Hop"`, etc.).
    pub category_covers: std::collections::HashMap<String, String>,
}

fn default_target_fps() -> u32 {
    120
}

fn default_true() -> bool {
    true
}

/// Mode de répétition de la file d'attente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl RepeatMode {
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

/// Police de caractères de l'interface utilisateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum UiFontFamily {
    /// Police actuelle par défaut (Inter / Segoe UI Variable).
    #[default]
    Default,
    /// Season Mix (Displaay).
    SeasonMix,
    /// DM Sans.
    DmSans,
    /// Plus Jakarta Sans.
    PlusJakartaSans,
}

impl UiFontFamily {
    pub const ALL: [Self; 4] = [
        Self::Default,
        Self::SeasonMix,
        Self::DmSans,
        Self::PlusJakartaSans,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "Inter (Actuelle)",
            Self::SeasonMix => "Season Mix",
            Self::DmSans => "DM Sans",
            Self::PlusJakartaSans => "Plus Jakarta Sans",
        }
    }
}

/// Mode de normalisation ReplayGain sans perte (protection crête 0 dBFS incluse).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ReplayGainMode {
    Off,
    #[default]
    Track,
    Album,
}

impl ReplayGainMode {
    pub const ALL: [Self; 3] = [Self::Off, Self::Track, Self::Album];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "Désactivé",
            Self::Track => "Piste",
            Self::Album => "Album",
        }
    }
}

/// Statistique d'écoute agrégée pour un titre.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ActivityTrackStat {
    pub track: TrackSummary,
    pub play_count: u32,
    pub total_secs: u64,
}

/// Statistique d'écoute agrégée pour un artiste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ActivityArtistStat {
    pub artist_name: String,
    pub artist_id: Option<u64>,
    pub play_count: u32,
    pub total_secs: u64,
}

/// Statistiques locales « Mon Activité » calculées depuis SQLite.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ListeningActivityStats {
    pub total_plays: u64,
    pub total_secs: u64,
    pub plays_last_30_days: u64,
    pub secs_last_30_days: u64,
    pub unique_artists: u64,
    pub top_tracks: Vec<ActivityTrackStat>,
    pub top_artists: Vec<ActivityArtistStat>,
    pub recent_plays: Vec<TrackSummary>,
}

/// Préférences audio & interface persistées dans le snapshot instantané.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AppPreferences {
    pub device_id: Option<String>,
    pub exclusive_mode: bool,
    pub fallback_to_shared: bool,
    pub bit_perfect_lock: bool,
    pub audio_quality: AudioQuality,
    /// Volume logiciel linéaire `0.0..=1.0` (ignoré si `bit_perfect_lock == true`).
    pub volume: f32,
    /// Normalisation du volume sans perte (ReplayGain + protection crête 0 dBFS, sans compression).
    #[serde(default = "default_true")]
    pub normalize_volume: bool,
    /// Mode ReplayGain (Désactivé / Piste / Album).
    #[serde(default)]
    pub replay_gain_mode: ReplayGainMode,
    /// Autoriser les contenus explicites (activé par défaut).
    #[serde(default = "default_true")]
    pub allow_explicit: bool,
    /// Autoriser les contenus générés par l'IA (désactivé par défaut).
    #[serde(default)]
    pub allow_ai_content: bool,
    /// Fréquence d'images cible (`60`, `120` ou `0` pour Auto).
    #[serde(default = "default_target_fps")]
    pub target_fps: u32,
    /// Mode de lecture aléatoire actif.
    #[serde(default)]
    pub shuffle_enabled: bool,
    /// Mode de répétition de la file d'attente.
    #[serde(default)]
    pub repeat_mode: RepeatMode,
    /// Enchaînement sans coupure entre deux pistes de format PCM identique.
    #[serde(default = "default_true")]
    pub gapless_playback: bool,
    /// Lance automatiquement la radio du titre à la fin de la file d'attente.
    #[serde(default = "default_true")]
    pub autoplay_radio: bool,
    /// Réduit l'application dans la zone de notification Windows à la fermeture.
    #[serde(default)]
    pub minimize_to_tray: bool,
    /// Active le scrobbling automatique Last.fm lorsqu'un compte est connecté.
    #[serde(default)]
    pub lastfm_enabled: bool,
    /// Police de caractères de l'interface (Inter actuelle, Season Mix, DM Sans, Plus Jakarta Sans).
    #[serde(default)]
    pub ui_font: UiFontFamily,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            device_id: None,
            exclusive_mode: true,
            fallback_to_shared: true,
            bit_perfect_lock: false,
            audio_quality: AudioQuality::HiResLossless,
            volume: 1.0,
            normalize_volume: true,
            replay_gain_mode: ReplayGainMode::Track,
            allow_explicit: true,
            allow_ai_content: false,
            target_fps: 120,
            shuffle_enabled: false,
            repeat_mode: RepeatMode::Off,
            gapless_playback: true,
            autoplay_radio: true,
            minimize_to_tray: false,
            lastfm_enabled: false,
            ui_font: UiFontFamily::Default,
        }
    }
}

/// État de lecture restauré au démarrage pour reprendre la file d'attente
/// exactement là où l'utilisateur l'avait laissée.
/// Les champs absents d'un snapshot plus ancien retombent sur leur valeur par défaut.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SessionPlaybackState {
    pub queue: Vec<TrackSummary>,
    pub original_queue: Vec<TrackSummary>,
    pub queue_index: Option<usize>,
    pub position_ms: u64,
}

/// Snapshot minimal lu au démarrage à froid (< 5 ms) pour afficher immédiatement la bibliothèque.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct UiSnapshot {
    pub user_id: Option<u64>,
    pub country_code: Option<String>,
    pub preferences: AppPreferences,
    pub favorites: Vec<TrackSummary>,
    pub playlists: Vec<PlaylistSummary>,
    #[serde(default)]
    pub playlist_folders: Vec<PlaylistFolder>,
    #[serde(default)]
    pub favorite_albums: Vec<AlbumSummary>,
    #[serde(default)]
    pub favorite_artists: Vec<ArtistSummary>,
    #[serde(default)]
    pub recent_searches: Vec<String>,
    pub recent_tracks: Vec<TrackSummary>,
    #[serde(default)]
    pub user_mixes: Vec<MixSummary>,
    #[serde(default)]
    pub discover_tracks: Vec<TrackSummary>,
    #[serde(default)]
    pub discover_refreshed_at: Option<u64>,
    #[serde(default)]
    pub discover_rotation: u64,
    /// File d'attente et position restaurées à la reprise d'une session.
    #[serde(default)]
    pub playback_session: Option<SessionPlaybackState>,
}

/// État courant du moteur de lecture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PlaybackState {
    Stopped,
    Buffering,
    Playing,
    Paused,
    Error(String),
}

/// Commandes envoyées par l'UI ou SMTC vers le coordinateur de lecture.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PlayerCommand {
    LoadTrack {
        track_id: u64,
        quality: AudioQuality,
    },
    LoadTrackAt {
        track_id: u64,
        quality: AudioQuality,
        start_position_ms: u64,
    },
    PrefetchTrack {
        track_id: u64,
        quality: AudioQuality,
    },
    Play,
    Pause,
    Stop,
    Seek {
        position_ms: u64,
    },
    SelectDevice {
        device_id: Option<String>,
        exclusive: bool,
    },
    SetVolume {
        /// Gain linéaire `0.0..=1.0` (ignoré si `bit_perfect_lock == true`).
        linear: f32,
        bit_perfect_lock: bool,
    },
    SetNormalization {
        enabled: bool,
    },
    /// Sélectionne le mode ReplayGain (Désactivé, Piste ou Album).
    SetReplayGainMode {
        mode: ReplayGainMode,
    },
    /// Prépare la piste suivante pour un enchaînement sans coupure (gapless).
    QueueGaplessNext {
        track_id: Option<u64>,
        quality: AudioQuality,
    },
    /// Active ou désactive l'enchaînement gapless.
    SetGapless {
        enabled: bool,
    },
}

/// Événements émis par le moteur audio vers l'interface graphique.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PlayerEvent {
    StateChanged(PlaybackState),
    FormatNegotiated(StreamFormatInfo),
    PositionUpdated {
        track_id: u64,
        position_ms: u64,
        duration_ms: u64,
    },
    BufferHealth {
        buffered_ms: u64,
        underruns: u64,
    },
    DevicesEnumerated(Vec<AudioDeviceDescriptor>),
    TrackFinished {
        track_id: u64,
    },
    /// Le moteur a enchaîné deux pistes sans redémarrer le flux WASAPI.
    TrackTransitioned {
        from_track_id: u64,
        to_track_id: u64,
        duration_ms: u64,
    },
}

fn default_row_colors() -> [String; 4] {
    [
        "#33F3FF".to_string(),
        "#6366F1".to_string(),
        "#EC4899".to_string(),
        "#F59E0B".to_string(),
    ]
}

/// Palette harmonisée extraite d'une pochette pour le fond ambiant OLED Dark
/// et la surbrillance multi-couleurs de la piste active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CoverPalette {
    pub primary: String,
    pub secondary: String,
    pub accent: String,
    pub surface_tint: String,
    #[serde(default = "default_row_colors")]
    pub row_colors: [String; 4],
}

impl Default for CoverPalette {
    fn default() -> Self {
        Self {
            primary: "#111116".to_string(),
            secondary: "#0A0B0F".to_string(),
            accent: "#33F3FF".to_string(),
            surface_tint: "#0B0D14".to_string(),
            row_colors: default_row_colors(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_audiophile_badge_correctly() {
        let info = StreamFormatInfo {
            codec: "FLAC".into(),
            source_pcm: PcmFormat::stereo(192_000, 24),
            dac_container_bits: 32,
            dac_valid_bits: 24,
            is_exclusive: true,
            is_bit_perfect: true,
            device_name: "Topping DX3 Pro+".into(),
        };
        assert_eq!(
            info.audiophile_badge(),
            "FLAC • 24-bit / 192 kHz • WASAPI Exclusif • Bit-Perfect"
        );

        let cd = PcmFormat::stereo(44_100, 16);
        assert_eq!(cd.short_label(), "16-bit / 44.1 kHz");
    }

    #[test]
    fn builds_tidal_cover_url() {
        let track = TrackSummary {
            id: 1,
            title: "Test".into(),
            artist_name: "Artist".into(),
            artist_id: Some(7),
            album_id: Some(42),
            album_title: "Album".into(),
            cover_id: Some("a1b2c3d4-e5f6-7890-abcd-ef1234567890".into()),
            duration_secs: 210,
            audio_quality: Some("HI_RES_LOSSLESS".into()),
            explicit: false,
            dolby_audio: false,
        };
        assert_eq!(
            track.cover_url(640).as_deref(),
            Some("https://resources.tidal.com/images/a1b2c3d4/e5f6/7890/abcd/ef1234567890/640x640.jpg")
        );
        assert_eq!(track.artist_id, Some(7));
    }

    #[test]
    fn builds_artist_picture_url_and_sort_labels() {
        let artist = ArtistSummary {
            id: 27,
            name: "Daft Punk".into(),
            picture_id: Some("a1b2c3d4-e5f6-7890-abcd-ef1234567890".into()),
        };
        assert_eq!(
            artist.picture_url(320).as_deref(),
            Some("https://resources.tidal.com/images/a1b2c3d4/e5f6/7890/abcd/ef1234567890/320x320.jpg")
        );
        assert_eq!(ArtistSummary::default().picture_url(320), None);

        let labels: Vec<&str> = TrackSortOrder::ALL.iter().map(|o| o.label()).collect();
        assert_eq!(
            labels,
            vec![
                "Ordre d'ajout",
                "Titre (A–Z)",
                "Artiste (A–Z)",
                "Album (A–Z)",
                "Durée",
            ]
        );
        assert_eq!(TrackSortOrder::default(), TrackSortOrder::Default);
    }

    #[test]
    fn mix_summary_image_url_accepts_uuid_and_full_url() {
        let uuid_mix = MixSummary {
            id: "daily".into(),
            title: "Ma Découverte Quotidienne".into(),
            subtitle: "Sélection personnalisée".into(),
            image_id: Some("a1b2c3d4-e5f6-7890-abcd-ef1234567890".into()),
            tracks: Vec::new(),
        };
        assert_eq!(
            uuid_mix.image_url(640).as_deref(),
            Some("https://resources.tidal.com/images/a1b2c3d4/e5f6/7890/abcd/ef1234567890/640x640.jpg")
        );

        let url_mix = MixSummary {
            image_id: Some("https://resources.tidal.com/images/aa/bb/320x320.jpg".into()),
            ..uuid_mix
        };
        assert_eq!(
            url_mix.image_url(1280).as_deref(),
            Some("https://resources.tidal.com/images/aa/bb/320x320.jpg")
        );
    }

    #[test]
    fn repeat_mode_cycles_off_all_one() {
        assert_eq!(RepeatMode::Off.next(), RepeatMode::All);
        assert_eq!(RepeatMode::All.next(), RepeatMode::One);
        assert_eq!(RepeatMode::One.next(), RepeatMode::Off);
    }

    #[test]
    fn app_preferences_default_playback_flags() {
        let prefs = AppPreferences::default();
        assert!(!prefs.shuffle_enabled);
        assert_eq!(prefs.repeat_mode, RepeatMode::Off);
    }

    /// Valeur scalaire minimale pour désérialiser `AppPreferences` sans dépendance JSON.
    enum TestValue {
        Bool(bool),
        F32(f32),
        U32(u32),
        Str(&'static str),
        None,
    }

    impl<'de> serde::de::IntoDeserializer<'de, serde::de::value::Error> for TestValue {
        type Deserializer = Self;

        fn into_deserializer(self) -> Self::Deserializer {
            self
        }
    }

    impl<'de> serde::Deserializer<'de> for TestValue {
        type Error = serde::de::value::Error;

        fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: serde::de::Visitor<'de>,
        {
            match self {
                Self::Bool(value) => visitor.visit_bool(value),
                Self::F32(value) => visitor.visit_f32(value),
                Self::U32(value) => visitor.visit_u32(value),
                Self::Str(value) => visitor.visit_str(value),
                Self::None => visitor.visit_none(),
            }
        }

        fn deserialize_enum<V>(
            self,
            name: &'static str,
            variants: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: serde::de::Visitor<'de>,
        {
            match self {
                Self::Str(value) => serde::de::value::StrDeserializer::<Self::Error>::new(value)
                    .deserialize_enum(name, variants, visitor),
                _ => Err(serde::de::Error::custom(
                    "variante d'enum attendue sous forme de chaîne",
                )),
            }
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
            bytes byte_buf option unit unit_struct newtype_struct seq tuple
            tuple_struct map struct identifier ignored_any
        }
    }

    /// Les snapshots écrits avant l'ajout de `shuffle_enabled` / `repeat_mode`
    /// doivent toujours se désérialiser avec les valeurs par défaut.
    #[test]
    fn app_preferences_deserializes_legacy_snapshot_without_new_fields() {
        let entries: Vec<(&'static str, TestValue)> = vec![
            ("device_id", TestValue::None),
            ("exclusive_mode", TestValue::Bool(true)),
            ("fallback_to_shared", TestValue::Bool(true)),
            ("bit_perfect_lock", TestValue::Bool(false)),
            ("audio_quality", TestValue::Str("HiResLossless")),
            ("volume", TestValue::F32(0.75)),
            ("normalize_volume", TestValue::Bool(true)),
            ("allow_explicit", TestValue::Bool(true)),
            ("allow_ai_content", TestValue::Bool(false)),
            ("target_fps", TestValue::U32(60)),
        ];
        let deserializer: serde::de::value::MapDeserializer<'_, _, serde::de::value::Error> =
            serde::de::value::MapDeserializer::new(entries.into_iter());

        let prefs = AppPreferences::deserialize(deserializer).expect("snapshot historique");
        assert!(!prefs.shuffle_enabled);
        assert_eq!(prefs.repeat_mode, RepeatMode::Off);
        assert_eq!(prefs.audio_quality, AudioQuality::HiResLossless);
        assert_eq!(prefs.target_fps, 60);
        assert!((prefs.volume - 0.75).abs() < f32::EPSILON);
        assert!(prefs.device_id.is_none());
        assert!(prefs.gapless_playback);
        assert!(prefs.autoplay_radio);
    }

    /// Un snapshot écrit avant l'ajout de `gapless_playback` / `autoplay_radio`
    /// (et des autres réglages récents) doit conserver les valeurs par défaut.
    #[test]
    fn app_preferences_legacy_snapshot_enables_gapless_and_autoplay() {
        let entries: Vec<(&'static str, TestValue)> = vec![
            ("device_id", TestValue::None),
            ("exclusive_mode", TestValue::Bool(true)),
            ("fallback_to_shared", TestValue::Bool(true)),
            ("bit_perfect_lock", TestValue::Bool(false)),
            ("audio_quality", TestValue::Str("HiResLossless")),
            ("volume", TestValue::F32(1.0)),
        ];
        let deserializer: serde::de::value::MapDeserializer<'_, _, serde::de::value::Error> =
            serde::de::value::MapDeserializer::new(entries.into_iter());

        let prefs = AppPreferences::deserialize(deserializer).expect("snapshot historique");
        assert!(prefs.gapless_playback);
        assert!(prefs.autoplay_radio);
        assert!(prefs.normalize_volume);
        assert!(prefs.allow_explicit);
        assert!(!prefs.allow_ai_content);
        assert_eq!(prefs.target_fps, 120);
        assert_eq!(prefs.repeat_mode, RepeatMode::Off);
    }

    /// Un snapshot écrit avant la Priorité 4 (zone de notification, ReplayGain,
    /// Last.fm) doit retomber sur des valeurs sûres : normalisation en mode
    /// Piste, fermeture franche de la fenêtre et scrobbling désactivé.
    #[test]
    fn app_preferences_legacy_snapshot_defaults_priority4_fields() {
        let entries: Vec<(&'static str, TestValue)> = vec![
            ("device_id", TestValue::None),
            ("exclusive_mode", TestValue::Bool(true)),
            ("fallback_to_shared", TestValue::Bool(true)),
            ("bit_perfect_lock", TestValue::Bool(false)),
            ("audio_quality", TestValue::Str("HiResLossless")),
            ("volume", TestValue::F32(1.0)),
            ("normalize_volume", TestValue::Bool(false)),
            ("allow_explicit", TestValue::Bool(true)),
            ("allow_ai_content", TestValue::Bool(false)),
            ("target_fps", TestValue::U32(120)),
        ];
        let deserializer: serde::de::value::MapDeserializer<'_, _, serde::de::value::Error> =
            serde::de::value::MapDeserializer::new(entries.into_iter());

        let prefs = AppPreferences::deserialize(deserializer).expect("snapshot historique");
        assert_eq!(prefs.replay_gain_mode, ReplayGainMode::Track);
        assert!(!prefs.minimize_to_tray);
        assert!(!prefs.lastfm_enabled);
        assert!(!prefs.normalize_volume);
    }

    #[test]
    fn replay_gain_mode_exposes_all_variants_and_stable_labels() {
        assert_eq!(
            ReplayGainMode::ALL,
            [
                ReplayGainMode::Off,
                ReplayGainMode::Track,
                ReplayGainMode::Album,
            ]
        );
        let labels: Vec<&str> = ReplayGainMode::ALL.iter().map(|mode| mode.label()).collect();
        assert_eq!(labels, vec!["Désactivé", "Piste", "Album"]);
        assert_eq!(ReplayGainMode::default(), ReplayGainMode::Track);
    }

    /// Un snapshot de session partiel (champs ajoutés ultérieurement) doit
    /// rester désérialisable grâce aux valeurs par défaut.
    #[test]
    fn session_playback_state_tolerates_partial_snapshots() {
        let entries: Vec<(&'static str, TestValue)> = vec![("position_ms", TestValue::U32(1_234))];
        let deserializer: serde::de::value::MapDeserializer<'_, _, serde::de::value::Error> =
            serde::de::value::MapDeserializer::new(entries.into_iter());

        let session =
            SessionPlaybackState::deserialize(deserializer).expect("session partielle");
        assert!(session.queue.is_empty());
        assert!(session.original_queue.is_empty());
        assert_eq!(session.queue_index, None);
        assert_eq!(session.position_ms, 1_234);
    }

    #[test]
    fn parses_lrc_centiseconds_milliseconds_and_whole_seconds() {
        let lines = parse_synced_lyrics(
            "[00:12.34]Hello world\n[01:02.345]Deuxième ligne\n[02:05]Sans fraction",
        );
        assert_eq!(
            lines,
            vec![
                LyricsLine {
                    time_ms: 12_340,
                    text: "Hello world".into()
                },
                LyricsLine {
                    time_ms: 62_345,
                    text: "Deuxième ligne".into()
                },
                LyricsLine {
                    time_ms: 125_000,
                    text: "Sans fraction".into()
                },
            ]
        );

        let long_fraction = parse_synced_lyrics("[00:01.2345]Précision");
        assert_eq!(long_fraction[0].time_ms, 1_234);
    }

    #[test]
    fn lrc_ignores_metadata_tags_empty_text_and_sorts() {
        let lrc = "[ar:Daft Punk]\n\
                   [ti:Giorgio by Moroder]\n\
                   [al:Random Access Memories]\n\
                   [by:Kedi]\n\
                   [offset:+120]\n\
                   [00:45.50][00:10.00]Refrain\n\
                   [00:20.00]   \n\
                   [00:05.00]Intro";
        let lines = parse_synced_lyrics(lrc);
        let times: Vec<u64> = lines.iter().map(|line| line.time_ms).collect();
        assert_eq!(times, vec![5_000, 10_000, 45_500]);
        assert!(lines.iter().all(|line| !line.text.trim().is_empty()));
        assert_eq!(lines[1].text, "Refrain");
        assert_eq!(lines[2].text, "Refrain");
    }

    #[test]
    fn lrc_without_timestamps_or_text_yields_no_lines() {
        assert!(parse_synced_lyrics("").is_empty());
        assert!(parse_synced_lyrics("Paroles sans horodatage").is_empty());
        assert!(parse_synced_lyrics("[00:10.00]").is_empty());
        assert!(parse_synced_lyrics("[ar:Artiste]").is_empty());
    }

    #[test]
    fn active_lyrics_index_tracks_position_across_lines() {
        let lines = parse_synced_lyrics("[00:10.00]A\n[00:20.00]B\n[00:30.00]C");
        assert_eq!(active_lyrics_index(&[], 30_000), None);
        assert_eq!(active_lyrics_index(&lines, 0), None);
        assert_eq!(active_lyrics_index(&lines, 9_999), None);
        assert_eq!(active_lyrics_index(&lines, 10_000), Some(0));
        assert_eq!(active_lyrics_index(&lines, 25_000), Some(1));
        assert_eq!(active_lyrics_index(&lines, 30_000), Some(2));
        assert_eq!(active_lyrics_index(&lines, 3_600_000), Some(2));
    }

    /// BOM Windows, retours chariot, horodatages multiples séparés par des espaces
    /// et minutes à plusieurs chiffres doivent être acceptés.
    #[test]
    fn lrc_parses_bom_space_separated_and_multi_digit_minutes() {
        let lrc = "\u{feff}[00:00.00]Intro\r\n\
                   [00:10.00] [00:45.50]Refrain\n\
                   [123:45.678]Épilogue";
        let lines = parse_synced_lyrics(lrc);
        let times: Vec<u64> = lines.iter().map(|line| line.time_ms).collect();
        assert_eq!(times, vec![0, 10_000, 45_500, 7_425_678]);
        assert_eq!(lines[0].text, "Intro");
        assert_eq!(lines[1].text, "Refrain");
        assert_eq!(lines[3].text, "Épilogue");
    }

    /// Horodatages négatifs, crochets non fermés et balises non numériques
    /// sont ignorés sans faire perdre les lignes valides de la même réponse.
    #[test]
    fn lrc_ignores_negative_unclosed_and_malformed_timestamps() {
        let lrc = "[-00:05.00]Négatif\n\
                   [00:12.00Sans fermeture\n\
                   [abc:12.00]Invalide\n\
                   [00:20.00]Valide";
        let lines = parse_synced_lyrics(lrc);
        assert_eq!(
            lines,
            vec![LyricsLine {
                time_ms: 20_000,
                text: "Valide".into()
            }]
        );
    }

    #[test]
    fn lrc_zero_timestamp_and_duplicate_times_stay_sorted() {
        let lines = parse_synced_lyrics("[00:00.00]Intro\n[00:00.00]Intro bis\n[00:05.00]Suite");
        assert_eq!(active_lyrics_index(&lines, 0), Some(1));
        assert_eq!(active_lyrics_index(&lines, 4_999), Some(1));
        assert_eq!(active_lyrics_index(&lines, 5_000), Some(2));
    }

    #[test]
    fn lrc_saturates_absurd_minutes_instead_of_panicking() {
        let lines = parse_synced_lyrics("[18446744073709551615:59.999]Max");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].time_ms, u64::MAX);
    }
}
