//! Parseur MPEG-DASH (`application/dash+xml`) ultra-léger dédié aux manifests Hi-Res
//! de TIDAL (`SegmentTemplate` + `SegmentTimeline`).
//!
//! Zéro dépendance XML lourde : extrait directement les attributs de
//! `<Representation>`, `<SegmentTemplate>` et la liste des entrées `<S>` dans
//! `<SegmentTimeline>`, puis déroule les répétitions (`r="N"`) et substitue
//! `$Number$`, `$RepresentationID$`, `$Bandwidth$` et `$Time$`.

use crate::{Result, StreamError};

/// Un segment média individuel issu du déroulement de `<SegmentTimeline>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashSegment {
    /// Numéro de séquence (`startNumber + index`, typiquement à partir de `1`).
    pub number: u64,
    /// URL complète prête à télécharger.
    pub url: String,
    /// Timestamp de départ en unités `timescale`.
    pub start_ticks: u64,
    /// Durée du segment en unités `timescale`.
    pub duration_ticks: u64,
}

/// Plan de téléchargement MPEG-DASH résolu pour la meilleure `<Representation>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashStreamPlan {
    /// Identifiant de la représentation (`id`).
    pub representation_id: String,
    /// Codec annoncé (`"flac"`, `"mp4a.40.2"`, etc.).
    pub codecs: String,
    /// Type MIME du conteneur (`"audio/mp4"`).
    pub mime_type: String,
    /// Débit binaire annoncé en bits/s.
    pub bandwidth: u64,
    /// Fréquence d'échantillonnage annoncée (`audioSamplingRate`), si présente.
    pub sample_rate: Option<u32>,
    /// Échelle de temps du `SegmentTemplate` (souvent égale à `sample_rate`, ex. `192000`).
    pub timescale: u64,
    /// URL du segment d'initialisation fMP4 (`init.mp4` contenant l'atome `moov`).
    pub init_url: String,
    /// Liste ordonnée des segments audio (`moof` + `mdat`).
    pub segments: Vec<DashSegment>,
}

impl DashStreamPlan {
    /// Durée totale estimée du flux en millisecondes.
    pub fn total_duration_ms(&self) -> u64 {
        if self.timescale == 0 {
            return 0;
        }
        let total_ticks: u64 = self.segments.iter().map(|s| s.duration_ticks).sum();
        (total_ticks * 1000) / self.timescale
    }

    /// Trouve l'index du segment contenant la position `position_ms`.
    pub fn segment_index_at_ms(&self, position_ms: u64) -> usize {
        if self.timescale == 0 || self.segments.is_empty() {
            return 0;
        }
        let target_ticks = (position_ms * self.timescale) / 1000;
        for (idx, seg) in self.segments.iter().enumerate() {
            if target_ticks < seg.start_ticks.saturating_add(seg.duration_ticks) {
                return idx;
            }
        }
        self.segments.len() - 1
    }
}

/// Parse un document XML MPD TIDAL et sélectionne la `<Representation>` au plus
/// haut débit (`bandwidth`).
pub fn parse_mpd(xml: &str) -> Result<DashStreamPlan> {
    let adaptation_mime = extract_tag_attr(xml, "AdaptationSet", "mimeType")
        .unwrap_or_else(|| "audio/mp4".to_string());
    let adaptation_codecs = extract_tag_attr(xml, "AdaptationSet", "codecs");
    let adaptation_rate = extract_tag_attr(xml, "AdaptationSet", "audioSamplingRate")
        .and_then(|v| v.parse::<u32>().ok());

    let reps = split_blocks(xml, "Representation");
    if reps.is_empty() {
        return Err(StreamError::InvalidMpd(
            "aucune balise <Representation> trouvée dans le MPD".into(),
        ));
    }

    let mut best_plan: Option<DashStreamPlan> = None;

    for (rep_open, rep_inner) in reps {
        let rep_id = extract_attr(rep_open, "id").unwrap_or_else(|| "0".into());
        let bandwidth = extract_attr(rep_open, "bandwidth")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let codecs = extract_attr(rep_open, "codecs")
            .or_else(|| adaptation_codecs.clone())
            .unwrap_or_else(|| "flac".into());
        let mime_type = extract_attr(rep_open, "mimeType").unwrap_or_else(|| adaptation_mime.clone());
        let sample_rate = extract_attr(rep_open, "audioSamplingRate")
            .and_then(|s| s.parse::<u32>().ok())
            .or(adaptation_rate);

        // SegmentTemplate peut être dans Representation ou au niveau AdaptationSet
        let tpl_scope = if rep_inner.contains("<SegmentTemplate") {
            rep_inner
        } else {
            xml
        };

        let tpl_open = find_tag_open(tpl_scope, "SegmentTemplate").ok_or_else(|| {
            StreamError::InvalidMpd("balise <SegmentTemplate> absente du MPD".into())
        })?;

        let timescale = extract_attr(tpl_open, "timescale")
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| sample_rate.map(u64::from))
            .unwrap_or(44_100)
            .max(1);

        let start_number = extract_attr(tpl_open, "startNumber")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(1);

        let init_tpl = extract_attr(tpl_open, "initialization").ok_or_else(|| {
            StreamError::InvalidMpd("attribut initialization manquant sur <SegmentTemplate>".into())
        })?;
        let media_tpl = extract_attr(tpl_open, "media").ok_or_else(|| {
            StreamError::InvalidMpd("attribut media manquant sur <SegmentTemplate>".into())
        })?;

        let init_url = expand_template(&init_tpl, &rep_id, bandwidth, start_number, 0);

        // Déroulement de <SegmentTimeline>
        let timeline_inner = extract_inner_block(tpl_scope, "SegmentTimeline").ok_or_else(|| {
            StreamError::InvalidMpd("bloc <SegmentTimeline> manquant dans le MPD".into())
        })?;

        let mut segments = Vec::new();
        let mut current_number = start_number;
        let mut current_ticks: u64 = 0;

        for s_tag in find_all_tag_opens(timeline_inner, "S") {
            if let Some(t) = extract_attr(s_tag, "t").and_then(|v| v.parse::<u64>().ok()) {
                current_ticks = t;
            }
            let d = extract_attr(s_tag, "d")
                .and_then(|v| v.parse::<u64>().ok())
                .ok_or_else(|| StreamError::InvalidMpd("élément <S> sans attribut d".into()))?;
            let r = extract_attr(s_tag, "r")
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);
            let count = if r > 0 { (r as u64) + 1 } else { 1 };

            for _ in 0..count {
                let url = expand_template(
                    &media_tpl,
                    &rep_id,
                    bandwidth,
                    current_number,
                    current_ticks,
                );
                segments.push(DashSegment {
                    number: current_number,
                    url,
                    start_ticks: current_ticks,
                    duration_ticks: d,
                });
                current_number += 1;
                current_ticks = current_ticks.saturating_add(d);
            }
        }

        if segments.is_empty() {
            return Err(StreamError::InvalidMpd(
                "aucun segment généré depuis <SegmentTimeline>".into(),
            ));
        }

        let plan = DashStreamPlan {
            representation_id: rep_id,
            codecs,
            mime_type,
            bandwidth,
            sample_rate,
            timescale,
            init_url,
            segments,
        };

        if best_plan
            .as_ref()
            .is_none_or(|prev| plan.bandwidth >= prev.bandwidth)
        {
            best_plan = Some(plan);
        }
    }

    best_plan.ok_or_else(|| StreamError::InvalidMpd("aucune représentation valide".into()))
}

fn expand_template(
    tpl: &str,
    rep_id: &str,
    bandwidth: u64,
    number: u64,
    time: u64,
) -> String {
    let unescaped = unescape_xml(tpl);
    unescaped
        .replace("$RepresentationID$", rep_id)
        .replace("$Bandwidth$", &bandwidth.to_string())
        .replace("$Number$", &number.to_string())
        .replace("$Time$", &time.to_string())
}

fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// Extrait la valeur d'un attribut `attr="value"` dans l'ouverture d'une balise.
fn extract_attr(tag_open: &str, attr: &str) -> Option<String> {
    let mut search_from = 0;
    while let Some(pos) = tag_open[search_from..].find(attr) {
        let abs = search_from + pos;
        let before_ok = abs == 0
            || tag_open.as_bytes()[abs - 1].is_ascii_whitespace()
            || tag_open.as_bytes()[abs - 1] == b'<';
        let after_idx = abs + attr.len();
        if before_ok && after_idx < tag_open.len() {
            let rest = tag_open[after_idx..].trim_start();
            if let Some(after_eq) = rest.strip_prefix('=') {
                let after_eq = after_eq.trim_start();
                if let Some(quote) = after_eq.chars().next()
                    && (quote == '"' || quote == '\'')
                {
                    let content = &after_eq[quote.len_utf8()..];
                    if let Some(end_q) = content.find(quote) {
                        return Some(unescaped_attr(&content[..end_q]));
                    }
                }
            }
        }
        search_from = after_idx;
    }
    None
}

fn unescaped_attr(raw: &str) -> String {
    unescape_xml(raw)
}

fn find_tag_open<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let needle = format!("<{tag}");
    let mut from = 0;
    while let Some(pos) = xml[from..].find(&needle) {
        let start = from + pos;
        let after = start + needle.len();
        if after < xml.len() {
            let next_b = xml.as_bytes()[after];
            if next_b.is_ascii_whitespace() || next_b == b'>' || next_b == b'/' {
                let end = xml[after..].find('>')? + after + 1;
                return Some(&xml[start..end]);
            }
        }
        from = after;
    }
    None
}

fn find_all_tag_opens<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let needle = format!("<{tag}");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(pos) = xml[from..].find(&needle) {
        let start = from + pos;
        let after = start + needle.len();
        if after < xml.len() {
            let next_b = xml.as_bytes()[after];
            if next_b.is_ascii_whitespace() || next_b == b'>' || next_b == b'/' {
                if let Some(rel_end) = xml[after..].find('>') {
                    let end = after + rel_end + 1;
                    out.push(&xml[start..end]);
                    from = end;
                    continue;
                }
            }
        }
        from = after;
    }
    out
}

fn extract_tag_attr(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let open = find_tag_open(xml, tag)?;
    extract_attr(open, attr)
}

fn extract_inner_block<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = find_tag_open(xml, tag)?;
    let open_end = (open.as_ptr() as usize) - (xml.as_ptr() as usize) + open.len();
    let close_needle = format!("</{tag}>");
    let close_rel = xml[open_end..].find(&close_needle)?;
    Some(&xml[open_end..open_end + close_rel])
}

fn split_blocks<'a>(xml: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let needle = format!("<{tag}");
    let close_needle = format!("</{tag}>");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(pos) = xml[from..].find(&needle) {
        let start = from + pos;
        let after = start + needle.len();
        if after >= xml.len() {
            break;
        }
        let next_b = xml.as_bytes()[after];
        if !(next_b.is_ascii_whitespace() || next_b == b'>' || next_b == b'/') {
            from = after;
            continue;
        }
        let Some(rel_end) = xml[after..].find('>') else {
            break;
        };
        let open_end = after + rel_end + 1;
        let open_slice = &xml[start..open_end];
        if open_slice.ends_with("/>") {
            out.push((open_slice, ""));
            from = open_end;
            continue;
        }
        if let Some(close_rel) = xml[open_end..].find(&close_needle) {
            let inner = &xml[open_end..open_end + close_rel];
            out.push((open_slice, inner));
            from = open_end + close_rel + close_needle.len();
        } else {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TIDAL_MPD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" mediaPresentationDuration="PT12.000S">
  <Period id="0">
    <AdaptationSet id="0" contentType="audio" mimeType="audio/mp4">
      <Representation id="1" codecs="flac" bandwidth="4608000" audioSamplingRate="192000">
        <SegmentTemplate timescale="192000"
                         initialization="https://sp-pr-cf.audio.tidal.com/mediatracks/init.mp4?token=abc&amp;exp=1"
                         media="https://sp-pr-cf.audio.tidal.com/mediatracks/$Number$.mp4?token=abc&amp;exp=1"
                         startNumber="1">
          <SegmentTimeline>
            <S d="768000" r="1"/>
            <S d="384000"/>
          </SegmentTimeline>
        </SegmentTemplate>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>"#;

    #[test]
    fn parses_tidal_hi_res_mpd_and_unrolls_timeline() {
        let plan = parse_mpd(SAMPLE_TIDAL_MPD).expect("MPD should parse");
        assert_eq!(plan.codecs, "flac");
        assert_eq!(plan.mime_type, "audio/mp4");
        assert_eq!(plan.sample_rate, Some(192_000));
        assert_eq!(plan.timescale, 192_000);
        assert_eq!(
            plan.init_url,
            "https://sp-pr-cf.audio.tidal.com/mediatracks/init.mp4?token=abc&exp=1"
        );
        // r="1" => 2 segments de 768000 ticks (4 s chacun) + 1 segment de 384000 ticks (2 s) = 3 segments (10 s)
        assert_eq!(plan.segments.len(), 3);
        assert_eq!(plan.segments[0].number, 1);
        assert_eq!(plan.segments[0].start_ticks, 0);
        assert_eq!(plan.segments[0].duration_ticks, 768_000);
        assert_eq!(
            plan.segments[0].url,
            "https://sp-pr-cf.audio.tidal.com/mediatracks/1.mp4?token=abc&exp=1"
        );
        assert_eq!(plan.segments[1].number, 2);
        assert_eq!(plan.segments[1].start_ticks, 768_000);
        assert_eq!(plan.segments[2].number, 3);
        assert_eq!(plan.segments[2].start_ticks, 1_536_000);
        assert_eq!(plan.segments[2].duration_ticks, 384_000);
        assert_eq!(plan.total_duration_ms(), 10_000);
        assert_eq!(plan.segment_index_at_ms(0), 0);
        assert_eq!(plan.segment_index_at_ms(3_999), 0);
        assert_eq!(plan.segment_index_at_ms(4_000), 1);
        assert_eq!(plan.segment_index_at_ms(8_500), 2);
    }
}
