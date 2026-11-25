import type { CoverPalette } from "@/bindings";

export type CoverSize = 80 | 160 | 320 | 640 | 1280;

export interface TrackRowColorPalette {
  /** 4 distinct vibrant hex colors extracted from the thumbnail */
  colors: [string, string, string, string];
  /** Brightest legible accent color for the active track title & index */
  titleColor: string;
}

const DEFAULT_ROW_COLORS: TrackRowColorPalette = {
  colors: ["#33F3FF", "#6366F1", "#EC4899", "#F59E0B"],
  titleColor: "#33F3FF",
};

export function coverUrl(
  coverId: string | null | undefined,
  size: CoverSize = 320,
): string | null {
  if (!coverId) return null;
  const trimmed = coverId.trim();
  if (!trimmed) return null;

  const encoded = encodeURIComponent(trimmed);
  return `http://tidal-cover.localhost/${encoded}/${size}`;
}

export function hexToRgba(hex: string | null | undefined, alpha: number): string {
  const cleaned = (hex ?? "#33F3FF").replace("#", "").trim();
  const safeAlpha = Math.max(0, Math.min(1, alpha));
  if (cleaned.length === 6) {
    const r = Number.parseInt(cleaned.slice(0, 2), 16);
    const g = Number.parseInt(cleaned.slice(2, 4), 16);
    const b = Number.parseInt(cleaned.slice(4, 6), 16);
    if (!Number.isNaN(r) && !Number.isNaN(g) && !Number.isNaN(b)) {
      return `rgba(${r}, ${g}, ${b}, ${safeAlpha})`;
    }
  }
  return `rgba(51, 243, 255, ${safeAlpha})`;
}

const rowColorsCache = new Map<string, TrackRowColorPalette>();
const pendingRowExtractions = new Map<string, Promise<TrackRowColorPalette | null>>();

export function getCachedTrackRowColors(
  coverId: string | null | undefined,
): TrackRowColorPalette | null {
  if (!coverId) return null;
  return rowColorsCache.get(coverId.trim()) ?? null;
}

function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  const rf = r / 255;
  const gf = g / 255;
  const bf = b / 255;
  const max = Math.max(rf, gf, bf);
  const min = Math.min(rf, gf, bf);
  const l = (max + min) * 0.5;
  const d = max - min;
  if (d < 1e-5) return [0, 0, l];
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h = 0;
  if (max === rf) {
    h = (gf - bf) / d + (gf < bf ? 6 : 0);
  } else if (max === gf) {
    h = (bf - rf) / d + 2;
  } else {
    h = (rf - gf) / d + 4;
  }
  return [h / 6, s, l];
}

function hslToRgb(h: number, s: number, l: number): [number, number, number] {
  if (s < 1e-5) {
    const v = Math.round(Math.max(0, Math.min(255, l * 255)));
    return [v, v, v];
  }
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const hue = (tInput: number) => {
    let t = tInput;
    if (t < 0) t += 1;
    if (t > 1) t -= 1;
    if (t < 1 / 6) return p + (q - p) * 6 * t;
    if (t < 0.5) return q;
    if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
    return p;
  };
  return [
    Math.round(Math.max(0, Math.min(255, hue(h + 1 / 3) * 255))),
    Math.round(Math.max(0, Math.min(255, hue(h) * 255))),
    Math.round(Math.max(0, Math.min(255, hue(h - 1 / 3) * 255))),
  ];
}

function toHex([r, g, b]: [number, number, number]): string {
  const h = (n: number) => n.toString(16).padStart(2, "0").toUpperCase();
  return `#${h(r)}${h(g)}${h(b)}`;
}

function rgbChroma([r, g, b]: [number, number, number]): number {
  return (Math.max(r, g, b) - Math.min(r, g, b)) / 255;
}

function makeVividRowColor(
  rgb: [number, number, number],
  targetMinL = 0.56,
  targetMaxL = 0.7,
): [number, number, number] {
  const [h, s, l] = rgbToHsl(rgb[0], rgb[1], rgb[2]);
  const chroma = rgbChroma(rgb);
  if (chroma < 0.04 || s < 0.07) {
    const v = Math.round(Math.max(185, Math.min(235, l * 255)));
    return [v, v, Math.min(255, v + 8)];
  }
  const boostedS = Math.max(0.72, Math.min(0.96, s * 1.25));
  const boostedL = Math.max(targetMinL, Math.min(targetMaxL, l < 0.5 ? l * 1.35 : l));
  return hslToRgb(h, boostedS, boostedL);
}

function hueDistance(h1: number, h2: number): number {
  const diff = Math.abs(h1 - h2);
  return Math.min(diff, 1 - diff);
}

function rgbDistance(a: [number, number, number], b: [number, number, number]): number {
  return Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
}

interface ColorCluster {
  sr: number;
  sg: number;
  sb: number;
  weight: number;
  rgb: [number, number, number];
  hue: number;
  sat: number;
  light: number;
  chroma: number;
  score: number;
}

/**
 * Extracts 4 distinct, vivid colors from an RGBA thumbnail buffer (width x height)
 * so that all major color families of the cover (background, subject, warm/cool accents)
 * are represented in the active track row highlight.
 */
export function computeRowColorsFromRgbaPixels(
  data: Uint8ClampedArray,
  width = 32,
  height = 32,
): TrackRowColorPalette | null {
  if (data.length < 4) return null;

  const rawBuckets = new Map<number, { sr: number; sg: number; sb: number; weight: number }>();
  const cx = (width - 1) * 0.5;
  const cy = (height - 1) * 0.5;
  const maxDist = Math.hypot(cx, cy) || 1;

  for (let i = 0; i < data.length; i += 4) {
    const a = data[i + 3];
    if (a < 128) continue;
    const r = data[i];
    const g = data[i + 1];
    const b = data[i + 2];

    const pxIndex = i >> 2;
    const x = pxIndex % width;
    const y = Math.floor(pxIndex / width);
    const normCenterDist = Math.hypot(x - cx, y - cy) / maxDist;
    // Give center pixels (subject/face/artwork) a 1.75x weight boost so flat edge backgrounds don't monopolize
    const spatialWeight = 1.0 + 0.75 * (1.0 - Math.min(1, normCenterDist));

    // 12-bit quantization
    const key = ((r >> 4) << 8) | ((g >> 4) << 4) | (b >> 4);
    const prev = rawBuckets.get(key);
    if (prev) {
      prev.sr += r * spatialWeight;
      prev.sg += g * spatialWeight;
      prev.sb += b * spatialWeight;
      prev.weight += spatialWeight;
    } else {
      rawBuckets.set(key, {
        sr: r * spatialWeight,
        sg: g * spatialWeight,
        sb: b * spatialWeight,
        weight: spatialWeight,
      });
    }
  }

  if (rawBuckets.size === 0) return null;

  // Convert buckets and merge close photographic gradient neighbors (within RGB distance 34)
  const initialClusters: ColorCluster[] = [];
  for (const b of rawBuckets.values()) {
    const rgb: [number, number, number] = [
      Math.round(b.sr / b.weight),
      Math.round(b.sg / b.weight),
      Math.round(b.sb / b.weight),
    ];
    const [hue, sat, light] = rgbToHsl(rgb[0], rgb[1], rgb[2]);
    const chroma = rgbChroma(rgb);
    initialClusters.push({
      sr: b.sr,
      sg: b.sg,
      sb: b.sb,
      weight: b.weight,
      rgb,
      hue,
      sat,
      light,
      chroma,
      score: 0,
    });
  }

  initialClusters.sort((a, b) => b.weight - a.weight);

  const merged: ColorCluster[] = [];
  for (const item of initialClusters) {
    const target = merged.find((m) => rgbDistance(m.rgb, item.rgb) < 34);
    if (target) {
      target.sr += item.sr;
      target.sg += item.sg;
      target.sb += item.sb;
      target.weight += item.weight;
      target.rgb = [
        Math.round(target.sr / target.weight),
        Math.round(target.sg / target.weight),
        Math.round(target.sb / target.weight),
      ];
      const [h, s, l] = rgbToHsl(target.rgb[0], target.rgb[1], target.rgb[2]);
      target.hue = h;
      target.sat = s;
      target.light = l;
      target.chroma = rgbChroma(target.rgb);
    } else {
      merged.push({ ...item });
    }
  }

  // Score each merged cluster by vibrancy (chroma + saturation) and damped area weight
  for (const c of merged) {
    if (c.light < 0.06 || c.light > 0.95) {
      c.score = c.weight ** 0.25 * 0.02;
      continue;
    }
    const lightBell = 1.0 - Math.min(0.75, Math.abs(c.light - 0.52) * 1.15);
    const vibrancy = c.chroma < 0.045 ? 0.08 : 0.25 + c.chroma ** 1.15 * 3.2 + c.sat * c.chroma * 1.2;
    c.score = vibrancy * lightBell * c.weight ** 0.36;
  }

  merged.sort((a, b) => b.score - a.score);

  // Greedily pick up to 4 distinct colors maximizing hue diversity and visual contrast
  const picked: ColorCluster[] = [];
  for (const minHueDiff of [0.11, 0.06, 0.025, 0.0]) {
    for (const candidate of merged) {
      if (picked.length >= 4) break;
      if (picked.includes(candidate)) continue;
      if (minHueDiff > 0 && candidate.chroma < 0.05) continue;
      const distinctEnough = picked.every(
        (p) =>
          hueDistance(p.hue, candidate.hue) >= minHueDiff &&
          rgbDistance(p.rgb, candidate.rgb) >= (minHueDiff > 0.05 ? 48 : 28),
      );
      if (distinctEnough) {
        picked.push(candidate);
      }
    }
    if (picked.length >= 4) break;
  }

  if (picked.length === 0) {
    return DEFAULT_ROW_COLORS;
  }

  // Fill remaining slots if the cover has fewer than 4 clusters by shifting hue/lightness harmoniously
  const vividHexes: string[] = picked.map((c) => toHex(makeVividRowColor(c.rgb)));
  while (vividHexes.length < 4) {
    const base = picked[vividHexes.length % picked.length];
    const shiftHue = (base.hue + 0.08 * vividHexes.length) % 1;
    const shifted = hslToRgb(
      shiftHue,
      Math.max(0.68, Math.min(0.92, base.sat)),
      Math.max(0.56, Math.min(0.68, base.light)),
    );
    vividHexes.push(toHex(shifted));
  }

  const titleRgb = makeVividRowColor(picked[0].rgb, 0.64, 0.76);

  return {
    colors: [vividHexes[0], vividHexes[1], vividHexes[2], vividHexes[3]],
    titleColor: toHex(titleRgb),
  };
}

export async function extractTrackRowColors(
  coverId: string | null | undefined,
): Promise<TrackRowColorPalette | null> {
  if (!coverId || typeof document === "undefined" || typeof Image === "undefined") {
    return null;
  }
  const key = coverId.trim();
  if (!key) return null;

  const cached = rowColorsCache.get(key);
  if (cached) return cached;

  const pending = pendingRowExtractions.get(key);
  if (pending) return pending;

  const url = coverUrl(key, 80);
  if (!url) return null;

  const task = new Promise<TrackRowColorPalette | null>((resolve) => {
    try {
      const img = new Image();
      img.crossOrigin = "anonymous";
      img.decoding = "async";
      img.onload = () => {
        try {
          const canvas = document.createElement("canvas");
          const size = 32;
          canvas.width = size;
          canvas.height = size;
          const ctx = canvas.getContext("2d");
          if (!ctx) {
            resolve(null);
            return;
          }
          ctx.drawImage(img, 0, 0, size, size);
          const imageData = ctx.getImageData(0, 0, size, size);
          const palette = computeRowColorsFromRgbaPixels(imageData.data, size, size);
          if (palette) {
            if (rowColorsCache.size >= 256) {
              rowColorsCache.clear();
            }
            rowColorsCache.set(key, palette);
          }
          resolve(palette);
        } catch {
          resolve(null);
        }
      };
      img.onerror = () => resolve(null);
      img.src = url;
    } catch {
      resolve(null);
    }
  }).finally(() => {
    pendingRowExtractions.delete(key);
  });

  pendingRowExtractions.set(key, task);
  return task;
}

export interface HeaderTintPalette {
  primary: string;
  secondary: string;
  accent: string;
}

const DEFAULT_HEADER_TINT: HeaderTintPalette = {
  primary: "#1D2436",
  secondary: "#121520",
  accent: "#33F3FF",
};

function hexToRgbTuple(hex: string): [number, number, number] | null {
  const cleaned = hex.replace("#", "").trim();
  if (cleaned.length !== 6) return null;
  const r = Number.parseInt(cleaned.slice(0, 2), 16);
  const g = Number.parseInt(cleaned.slice(2, 4), 16);
  const b = Number.parseInt(cleaned.slice(4, 6), 16);
  if (Number.isNaN(r) || Number.isNaN(g) || Number.isNaN(b)) return null;
  return [r, g, b];
}

function harmonizeHeaderColor(hex: string, targetL: number, maxSat: number): string {
  const rgb = hexToRgbTuple(hex);
  if (!rgb) return DEFAULT_HEADER_TINT.primary;
  const [h, s] = rgbToHsl(rgb[0], rgb[1], rgb[2]);
  const tunedS = s < 0.08 ? s : Math.min(maxSat, Math.max(0.32, s * 0.85));
  return toHex(hslToRgb(h, tunedS, targetL));
}

export function deriveHeaderTint(
  palette: TrackRowColorPalette | null | undefined,
): HeaderTintPalette {
  if (!palette) return DEFAULT_HEADER_TINT;
  return {
    primary: harmonizeHeaderColor(palette.colors[0], 0.36, 0.72),
    secondary: harmonizeHeaderColor(palette.colors[1] ?? palette.colors[0], 0.26, 0.62),
    accent: palette.titleColor || DEFAULT_HEADER_TINT.accent,
  };
}

export function getCachedCoverPalette(_coverId: string | null | undefined): CoverPalette | null {
  return null;
}

export async function extractCoverPaletteFromUrl(
  _coverId: string | null | undefined,
): Promise<CoverPalette | null> {
  return null;
}
