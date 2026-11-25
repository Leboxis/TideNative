//! Export et import de playlists au format CSV (RFC 4180) et M3U8 (`#EXTM3U`).

use serde::{Deserialize, Serialize};
use tidal_core::{PlaylistSummary, TrackSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PlaylistExportFormat {
    Csv,
    M3u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlaylistExportPayload {
    pub filename: String,
    pub content: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlaylistImportResult {
    pub playlist: PlaylistSummary,
    pub imported_count: u32,
    pub unmatched: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportTrackEntry {
    pub track_id: Option<u64>,
    pub query: String,
    pub label: String,
}

pub fn export_playlist(
    title: &str,
    tracks: &[TrackSummary],
    format: PlaylistExportFormat,
) -> PlaylistExportPayload {
    let safe_stem = sanitize_filename(title);
    match format {
        PlaylistExportFormat::Csv => {
            let mut out = String::from("track_id,title,artist,album,duration_secs,tidal_url\n");
            for t in tracks {
                let url = format!("https://tidal.com/browse/track/{}", t.id);
                out.push_str(&format!(
                    "{},{},{},{},{},{}\n",
                    t.id,
                    csv_escape(&t.title),
                    csv_escape(&t.artist_name),
                    csv_escape(&t.album_title),
                    t.duration_secs,
                    url
                ));
            }
            PlaylistExportPayload {
                filename: format!("{safe_stem}.csv"),
                content: out,
                mime_type: "text/csv;charset=utf-8".into(),
            }
        }
        PlaylistExportFormat::M3u8 => {
            let mut out = String::from("#EXTM3U\n");
            let clean_title = title.replace(['\r', '\n'], " ");
            if !clean_title.trim().is_empty() {
                out.push_str(&format!("#PLAYLIST:{}\n", clean_title.trim()));
            }
            for t in tracks {
                let artist = t.artist_name.replace(['\r', '\n'], " ");
                let name = t.title.replace(['\r', '\n'], " ");
                out.push_str(&format!("#EXTINF:{},{} - {}\n", t.duration_secs, artist, name));
                out.push_str(&format!("https://tidal.com/browse/track/{}\n", t.id));
            }
            PlaylistExportPayload {
                filename: format!("{safe_stem}.m3u8"),
                content: out,
                mime_type: "audio/x-mpegurl;charset=utf-8".into(),
            }
        }
    }
}

pub fn parse_playlist_import(content: &str, format: PlaylistExportFormat) -> Vec<ImportTrackEntry> {
    let cleaned = content.trim_start_matches('\u{feff}');
    match format {
        PlaylistExportFormat::Csv => parse_csv_import(cleaned),
        PlaylistExportFormat::M3u8 => parse_m3u8_import(cleaned),
    }
}

fn parse_csv_import(content: &str) -> Vec<ImportTrackEntry> {
    let mut rows = parse_csv_rows(content).into_iter().filter(|r| {
        r.iter().any(|cell| !cell.trim().is_empty())
    });
    let Some(first_row) = rows.next() else {
        return Vec::new();
    };

    let lower_first: Vec<String> = first_row
        .iter()
        .map(|c| c.trim().to_ascii_lowercase())
        .collect();
    let has_header = lower_first.iter().any(|h| {
        matches!(
            h.as_str(),
            "title" | "titre" | "track" | "track_id" | "id" | "artist" | "artiste" | "tidal_url" | "url"
        )
    });

    let (id_col, title_col, artist_col, url_col) = if has_header {
        let find = |names: &[&str]| {
            lower_first
                .iter()
                .position(|h| names.iter().any(|n| h == n))
        };
        (
            find(&["track_id", "id", "tidal_id"]),
            find(&["title", "titre", "track", "track_name", "name"]),
            find(&["artist", "artiste", "artist_name", "artists"]),
            find(&["tidal_url", "url", "link"]),
        )
    } else {
        (None, Some(0), Some(1), None)
    };

    let mut entries = Vec::new();
    let data_iter: Box<dyn Iterator<Item = Vec<String>>> = if has_header {
        Box::new(rows)
    } else {
        Box::new(std::iter::once(first_row).chain(rows))
    };

    for row in data_iter {
        let get = |idx: Option<usize>| -> String {
            idx.and_then(|i| row.get(i))
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };
        let raw_id = get(id_col);
        let raw_url = get(url_col);
        let title = get(title_col);
        let artist = get(artist_col);

        let track_id = raw_id
            .parse::<u64>()
            .ok()
            .filter(|&id| id > 0)
            .or_else(|| extract_tidal_track_id(&raw_url))
            .or_else(|| extract_tidal_track_id(&title));

        let query = match (artist.is_empty(), title.is_empty()) {
            (false, false) => format!("{artist} {title}"),
            (true, false) => title.clone(),
            (false, true) => artist.clone(),
            (true, true) => String::new(),
        };
        let label = match (artist.is_empty(), title.is_empty()) {
            (false, false) => format!("{artist} - {title}"),
            (true, false) => title,
            (false, true) => artist,
            (true, true) => track_id.map(|id| format!("Track #{id}")).unwrap_or_default(),
        };

        if track_id.is_some() || !query.is_empty() {
            entries.push(ImportTrackEntry {
                track_id,
                query,
                label,
            });
        }
    }

    entries
}

fn parse_m3u8_import(content: &str) -> Vec<ImportTrackEntry> {
    let mut entries = Vec::new();
    let mut pending_extinf: Option<String> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            if let Some((_, label)) = rest.split_once(',') {
                let trimmed = label.trim();
                if !trimmed.is_empty() {
                    pending_extinf = Some(trimmed.to_string());
                }
            }
            continue;
        }
        if line.starts_with('#') {
            continue;
        }

        let track_id = extract_tidal_track_id(line);
        let extinf = pending_extinf.take();
        let query = extinf
            .clone()
            .unwrap_or_else(|| {
                if track_id.is_some() {
                    String::new()
                } else {
                    extract_filename_stem(line)
                }
            });
        let label = extinf
            .or_else(|| track_id.map(|id| format!("Track #{id}")))
            .unwrap_or_else(|| query.clone());

        if track_id.is_some() || !query.is_empty() {
            entries.push(ImportTrackEntry {
                track_id,
                query,
                label,
            });
        }
    }

    if let Some(dangling) = pending_extinf {
        entries.push(ImportTrackEntry {
            track_id: None,
            query: dangling.clone(),
            label: dangling,
        });
    }

    entries
}

pub fn extract_tidal_track_id(text: &str) -> Option<u64> {
    let lower = text.to_ascii_lowercase();
    for marker in ["/track/", "tidal:track:"] {
        if let Some(pos) = lower.find(marker) {
            let after = &text[pos + marker.len()..];
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(id) = digits.parse::<u64>() {
                if id > 0 {
                    return Some(id);
                }
            }
        }
    }
    None
}

fn extract_filename_stem(path_or_url: &str) -> String {
    let last = path_or_url
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path_or_url);
    let stem = match last.rsplit_once('.') {
        Some((base, _)) if !base.is_empty() => base,
        _ => last,
    };
    stem.replace(['_', '-'], " ").trim().to_string()
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "playlist".into()
    } else {
        trimmed.chars().take(64).collect()
    }
}

fn csv_escape(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn parse_csv_rows(input: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut current_row = Vec::new();
    let mut current_cell = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    current_cell.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                current_cell.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => {
                    current_row.push(std::mem::take(&mut current_cell));
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    current_row.push(std::mem::take(&mut current_cell));
                    rows.push(std::mem::take(&mut current_row));
                }
                '\n' => {
                    current_row.push(std::mem::take(&mut current_cell));
                    rows.push(std::mem::take(&mut current_row));
                }
                _ => current_cell.push(c),
            }
        }
    }

    if !current_cell.is_empty() || !current_row.is_empty() {
        current_row.push(current_cell);
        rows.push(current_row);
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tracks() -> Vec<TrackSummary> {
        vec![
            TrackSummary {
                id: 1001,
                title: "Get Lucky (feat. Pharrell, Nile)".into(),
                artist_name: "Daft Punk".into(),
                artist_id: Some(10),
                album_id: Some(20),
                album_title: "Random Access Memories".into(),
                cover_id: None,
                duration_secs: 369,
                audio_quality: Some("HI_RES_LOSSLESS".into()),
                explicit: false,
                dolby_audio: false,
            },
            TrackSummary {
                id: 1002,
                title: "Instant \"Crush\"".into(),
                artist_name: "Daft Punk".into(),
                artist_id: Some(10),
                album_id: Some(20),
                album_title: "Random Access Memories".into(),
                cover_id: None,
                duration_secs: 337,
                audio_quality: None,
                explicit: false,
                dolby_audio: false,
            },
        ]
    }

    #[test]
    fn csv_export_and_import_roundtrip_handles_commas_and_quotes() {
        let tracks = sample_tracks();
        let exported = export_playlist("Mes Favoris / 2026", &tracks, PlaylistExportFormat::Csv);
        assert_eq!(exported.filename, "Mes Favoris _ 2026.csv");
        let parsed = parse_playlist_import(&exported.content, PlaylistExportFormat::Csv);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].track_id, Some(1001));
        assert!(parsed[0].query.contains("Get Lucky (feat. Pharrell, Nile)"));
        assert_eq!(parsed[1].track_id, Some(1002));
        assert!(parsed[1].query.contains("Instant \"Crush\""));
    }

    #[test]
    fn m3u8_export_and_import_roundtrip_and_third_party_fallback() {
        let tracks = sample_tracks();
        let exported = export_playlist("Road Trip", &tracks, PlaylistExportFormat::M3u8);
        assert_eq!(exported.filename, "Road Trip.m3u8");
        assert!(exported.content.starts_with("#EXTM3U\n#PLAYLIST:Road Trip\n"));

        let parsed = parse_playlist_import(&exported.content, PlaylistExportFormat::M3u8);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].track_id, Some(1001));
        assert_eq!(parsed[0].label, "Daft Punk - Get Lucky (feat. Pharrell, Nile)");

        let external = "#EXTM3U\n#EXTINF:210,Justice - D.A.N.C.E.\n/music/dance.flac\n";
        let ext_parsed = parse_playlist_import(external, PlaylistExportFormat::M3u8);
        assert_eq!(ext_parsed.len(), 1);
        assert_eq!(ext_parsed[0].track_id, None);
        assert_eq!(ext_parsed[0].query, "Justice - D.A.N.C.E.");
    }
}
