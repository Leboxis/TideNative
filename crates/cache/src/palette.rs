use std::collections::HashMap;
use tidal_core::CoverPalette;
use zune_jpeg::JpegDecoder;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

const MAX_OLED_LUMINANCE: f32 = 0.42;

#[derive(Clone, Debug)]
struct ColorCluster {
    sr: f64,
    sg: f64,
    sb: f64,
    weight: f64,
    rgb: [u8; 3],
    hue: f32,
    sat: f32,
    light: f32,
    chroma: f32,
    score: f32,
}

pub fn extract_cover_palette(jpeg_bytes: &[u8]) -> CoverPalette {
    if jpeg_bytes.is_empty() {
        return CoverPalette::default();
    }

    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = JpegDecoder::new_with_options(std::io::Cursor::new(jpeg_bytes), options);
    let Ok(pixels) = decoder.decode() else {
        return CoverPalette::default();
    };
    if pixels.len() < 3 {
        return CoverPalette::default();
    }

    let total_pixels = pixels.len() / 3;
    let (img_w, img_h) = decoder
        .dimensions()
        .unwrap_or_else(|| {
            let side = (total_pixels as f64).sqrt().round().max(1.0) as usize;
            (side, side)
        });
    let img_w = img_w.max(1);
    let img_h = img_h.max(1);
    let cx = (img_w.saturating_sub(1)) as f32 * 0.5;
    let cy = (img_h.saturating_sub(1)) as f32 * 0.5;
    let max_center_dist = cx.hypot(cy).max(1.0);

    let stride = (total_pixels / 1400).max(1);
    let mut buckets: HashMap<u16, (f64, f64, f64, f64)> = HashMap::new();

    for idx in (0..total_pixels).step_by(stride) {
        let base = idx * 3;
        let r = pixels[base];
        let g = pixels[base + 1];
        let b = pixels[base + 2];

        let x = (idx % img_w) as f32;
        let y = (idx / img_w) as f32;
        let norm_dist = (x - cx).hypot(y - cy) / max_center_dist;
        // Favorise le sujet central de la pochette par rapport au fond périphérique uniforme.
        let spatial_weight = f64::from(1.0 + 0.80 * (1.0 - norm_dist.min(1.0)));

        let key = (((r as u16) >> 4) << 8) | (((g as u16) >> 4) << 4) | ((b as u16) >> 4);
        let entry = buckets.entry(key).or_insert((0.0, 0.0, 0.0, 0.0));
        entry.0 += f64::from(r) * spatial_weight;
        entry.1 += f64::from(g) * spatial_weight;
        entry.2 += f64::from(b) * spatial_weight;
        entry.3 += spatial_weight;
    }

    if buckets.is_empty() {
        return CoverPalette::default();
    }

    let mut initial: Vec<ColorCluster> = buckets
        .into_values()
        .map(|(sr, sg, sb, w)| {
            let rgb = [
                (sr / w).round().clamp(0.0, 255.0) as u8,
                (sg / w).round().clamp(0.0, 255.0) as u8,
                (sb / w).round().clamp(0.0, 255.0) as u8,
            ];
            let (hue, sat, light) = rgb_to_hsl(rgb[0], rgb[1], rgb[2]);
            let chroma = rgb_chroma(rgb);
            ColorCluster {
                sr,
                sg,
                sb,
                weight: w,
                rgb,
                hue,
                sat,
                light,
                chroma,
                score: 0.0,
            }
        })
        .collect();

    initial.sort_by(|a, b| b.weight.total_cmp(&a.weight));

    // Fusionne les buckets adjacents (dégradés photographiques de peau, cheveux, flammes, ciel...)
    // pour qu'un sujet riche en nuances ne soit pas écrasé par un fond plat uniforme.
    let mut merged: Vec<ColorCluster> = Vec::new();
    for item in initial {
        if let Some(target) = merged
            .iter_mut()
            .find(|m| color_distance(m.rgb, item.rgb) < 34.0)
        {
            target.sr += item.sr;
            target.sg += item.sg;
            target.sb += item.sb;
            target.weight += item.weight;
            target.rgb = [
                (target.sr / target.weight).round().clamp(0.0, 255.0) as u8,
                (target.sg / target.weight).round().clamp(0.0, 255.0) as u8,
                (target.sb / target.weight).round().clamp(0.0, 255.0) as u8,
            ];
            let (h, s, l) = rgb_to_hsl(target.rgb[0], target.rgb[1], target.rgb[2]);
            target.hue = h;
            target.sat = s;
            target.light = l;
            target.chroma = rgb_chroma(target.rgb);
        } else {
            merged.push(item);
        }
    }

    for c in &mut merged {
        if !(0.06..=0.95).contains(&c.light) {
            c.score = (c.weight as f32).powf(0.25) * 0.02;
            continue;
        }
        let light_bell = 1.0 - ((c.light - 0.52).abs() * 1.15).min(0.75);
        let vibrancy = if c.chroma < 0.045 {
            0.08
        } else {
            0.25 + c.chroma.powf(1.15) * 3.2 + c.sat * c.chroma * 1.2
        };
        c.score = vibrancy * light_bell * (c.weight as f32).powf(0.36);
    }

    merged.sort_by(|a, b| b.score.total_cmp(&a.score));

    // Sélectionne jusqu'à 4 familles de couleurs distinctes de la pochette
    let mut picked: Vec<ColorCluster> = Vec::with_capacity(4);
    for min_hue_diff in [0.11_f32, 0.06, 0.025, 0.0] {
        for cand in &merged {
            if picked.len() >= 4 {
                break;
            }
            if min_hue_diff > 0.0 && cand.chroma < 0.05 {
                continue;
            }
            let min_rgb_dist = if min_hue_diff > 0.05 { 46.0 } else { 26.0 };
            let distinct = picked.iter().all(|p| {
                hue_distance(p.hue, cand.hue) >= min_hue_diff
                    && color_distance(p.rgb, cand.rgb) >= min_rgb_dist
            });
            if distinct {
                picked.push(cand.clone());
            }
        }
        if picked.len() >= 4 {
            break;
        }
    }

    if picked.is_empty() {
        picked.push(merged[0].clone());
    }

    let primary_rgb = picked[0].rgb;
    let secondary_rgb = picked
        .get(1)
        .map(|c| c.rgb)
        .unwrap_or_else(|| shift_tone(primary_rgb, -0.08, 0.85));

    let mut row_colors_vec: Vec<String> = picked
        .iter()
        .map(|c| to_hex(make_vivid_row_color(c.rgb, 0.56, 0.70)))
        .collect();
    while row_colors_vec.len() < 4 {
        let base = &picked[row_colors_vec.len() % picked.len()];
        let shifted_hue = (base.hue + 0.07 * (row_colors_vec.len() as f32)).rem_euclid(1.0);
        let shifted = hsl_to_rgb(
            shifted_hue,
            base.sat.clamp(0.68, 0.92),
            base.light.clamp(0.56, 0.68),
        );
        row_colors_vec.push(to_hex(shifted));
    }

    let primary = clamp_oled_luminance(primary_rgb, 0.34);
    let secondary = clamp_oled_luminance(secondary_rgb, 0.24);
    let surface_tint = clamp_oled_luminance(primary_rgb, 0.14);
    let accent = to_hex(make_vivid_row_color(primary_rgb, 0.60, 0.72));

    CoverPalette {
        primary: to_hex(primary),
        secondary: to_hex(secondary),
        accent,
        surface_tint: to_hex(surface_tint),
        row_colors: [
            row_colors_vec[0].clone(),
            row_colors_vec[1].clone(),
            row_colors_vec[2].clone(),
            row_colors_vec[3].clone(),
        ],
    }
}

fn hue_distance(h1: f32, h2: f32) -> f32 {
    let diff = (h1 - h2).abs();
    diff.min(1.0 - diff)
}

fn color_distance(a: [u8; 3], b: [u8; 3]) -> f32 {
    let dr = f32::from(a[0]) - f32::from(b[0]);
    let dg = f32::from(a[1]) - f32::from(b[1]);
    let db = f32::from(a[2]) - f32::from(b[2]);
    (dr * dr + dg * dg + db * db).sqrt()
}

fn shift_tone(rgb: [u8; 3], delta_l: f32, sat_scale: f32) -> [u8; 3] {
    let (h, s, l) = rgb_to_hsl(rgb[0], rgb[1], rgb[2]);
    hsl_to_rgb(h, (s * sat_scale).clamp(0.0, 1.0), (l + delta_l).clamp(0.05, 0.40))
}

fn clamp_oled_luminance(rgb: [u8; 3], target_max_l: f32) -> [u8; 3] {
    let (h, s, l) = rgb_to_hsl(rgb[0], rgb[1], rgb[2]);
    let mut clamped_l = l.min(target_max_l).max(0.07);
    let mut out = hsl_to_rgb(h, s.min(0.72), clamped_l);
    while relative_luminance(out) > MAX_OLED_LUMINANCE && clamped_l > 0.04 {
        clamped_l -= 0.03;
        out = hsl_to_rgb(h, s.min(0.72), clamped_l);
    }
    out
}

fn rgb_chroma(rgb: [u8; 3]) -> f32 {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    f32::from(max - min) / 255.0
}

fn make_vivid_row_color(rgb: [u8; 3], min_l: f32, max_l: f32) -> [u8; 3] {
    let (h, s, l) = rgb_to_hsl(rgb[0], rgb[1], rgb[2]);
    let chroma = rgb_chroma(rgb);
    if chroma < 0.04 || s < 0.07 {
        return [0xE2, 0xE6, 0xF0];
    }
    let boosted_s = (s * 1.25).clamp(0.72, 0.96);
    let boosted_l = (if l < 0.5 { l * 1.35 } else { l }).clamp(min_l, max_l);
    hsl_to_rgb(h, boosted_s, boosted_l)
}

fn relative_luminance(rgb: [u8; 3]) -> f32 {
    0.2126 * (f32::from(rgb[0]) / 255.0)
        + 0.7152 * (f32::from(rgb[1]) / 255.0)
        + 0.0722 * (f32::from(rgb[2]) / 255.0)
}

fn to_hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = f32::from(r) / 255.0;
    let gf = f32::from(g) / 255.0;
    let bf = f32::from(b) / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) * 0.5;
    let d = max - min;
    if d.abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let mut h = if (max - rf).abs() < f32::EPSILON {
        (gf - bf) / d + if gf < bf { 6.0 } else { 0.0 }
    } else if (max - gf).abs() < f32::EPSILON {
        (bf - rf) / d + 2.0
    } else {
        (rf - gf) / d + 4.0
    };
    h /= 6.0;
    (h, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [u8; 3] {
    if s.abs() < f32::EPSILON {
        let v = (l * 255.0).round().clamp(0.0, 255.0) as u8;
        return [v, v, v];
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue = |mut t: f32| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [
        (hue(h + 1.0 / 3.0) * 255.0).round().clamp(0.0, 255.0) as u8,
        (hue(h) * 255.0).round().clamp(0.0, 255.0) as u8,
        (hue(h - 1.0 / 3.0) * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_solid_jpeg_bytes(r: u8, g: u8, b: u8) -> Vec<u8> {
        let mut out = Vec::new();
        let pixels: Vec<u8> = std::iter::repeat_n([r, g, b], 16 * 16).flatten().collect();
        let encoder = jpeg_encoder::Encoder::new(&mut out, 90);
        encoder
            .encode(&pixels, 16, 16, jpeg_encoder::ColorType::Rgb)
            .expect("encode solid test jpeg");
        out
    }

    fn relative_luminance_hex(hex: &str) -> f32 {
        let s = hex.trim_start_matches('#');
        let r = u8::from_str_radix(&s[0..2], 16).unwrap() as f32 / 255.0;
        let g = u8::from_str_radix(&s[2..4], 16).unwrap() as f32 / 255.0;
        let b = u8::from_str_radix(&s[4..6], 16).unwrap() as f32 / 255.0;
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    #[test]
    fn fallback_palette_on_invalid_or_empty_bytes() {
        let p = extract_cover_palette(b"not-a-jpeg");
        assert_eq!(p, CoverPalette::default());
        assert_eq!(extract_cover_palette(&[]), CoverPalette::default());
    }

    #[test]
    fn clamps_pure_white_cover_luminance_for_oled_contrast() {
        let white_jpeg = test_solid_jpeg_bytes(255, 255, 255);
        let p = extract_cover_palette(&white_jpeg);
        for hex in [&p.primary, &p.secondary, &p.surface_tint] {
            assert!(relative_luminance_hex(hex) <= 0.42, "too bright: {hex}");
        }
    }

    #[test]
    fn extracts_distinct_saturated_primary_from_colored_jpeg() {
        let cyan_jpeg = test_solid_jpeg_bytes(20, 180, 220);
        let p = extract_cover_palette(&cyan_jpeg);
        assert_ne!(p.primary, CoverPalette::default().primary);
    }

    #[test]
    fn extracts_four_distinct_row_colors_from_multicolor_cover() {
        let mut pixels = Vec::with_capacity(32 * 32 * 3);
        for y in 0..32 {
            for x in 0..32 {
                let rgb = match (x < 16, y < 16) {
                    (true, true) => [70, 175, 225],   // Bleu ciel
                    (false, true) => [215, 135, 50],  // Ambre doré (cheveux)
                    (true, false) => [240, 165, 130], // Pêche chaud (visage)
                    (false, false) => [225, 50, 65],  // Rouge corail (sourire)
                };
                pixels.extend_from_slice(&rgb);
            }
        }
        let mut out = Vec::new();
        jpeg_encoder::Encoder::new(&mut out, 95)
            .encode(&pixels, 32, 32, jpeg_encoder::ColorType::Rgb)
            .expect("encode 4-color jpeg");
        let p = extract_cover_palette(&out);
        let unique: std::collections::HashSet<&String> = p.row_colors.iter().collect();
        assert_eq!(unique.len(), 4, "expected 4 distinct row colors: {:?}", p.row_colors);
    }
}
