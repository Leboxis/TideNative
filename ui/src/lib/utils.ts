import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function formatSecs(totalSecs: number): string {
  const safe = Math.max(0, Math.floor(totalSecs));
  const h = Math.floor(safe / 3600);
  const m = Math.floor((safe % 3600) / 60);
  const s = safe % 60;
  if (h > 0) {
    return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
  return `${m}:${String(s).padStart(2, "0")}`;
}

export function formatDurationMs(ms: number): string {
  return formatSecs(Math.floor(Math.max(0, ms) / 1000));
}

export type TidalQualityTier = "MAX" | "HIGH" | "LOW";

export function qualityTierFromApi(quality: string | null | undefined): TidalQualityTier {
  const upper = (quality ?? "").toUpperCase();
  if (upper.includes("HI_RES") || upper.includes("HIRES") || upper.includes("MASTER")) {
    return "MAX";
  }
  if (upper.includes("LOSSLESS") || upper === "HIGH") {
    return "HIGH";
  }
  return "LOW";
}

export function isDolbyAudio(
  quality: string | null | undefined,
  codec?: string | null,
): boolean {
  const haystack = `${quality ?? ""} ${codec ?? ""}`.toUpperCase();
  return (
    haystack.includes("DOLBY") ||
    haystack.includes("ATMOS") ||
    haystack.includes("AC4") ||
    haystack.includes("EC3") ||
    haystack.includes("EAC3")
  );
}

export function hasDolbyAudio(
  track: { dolby_audio?: boolean; audio_quality?: string | null } | null | undefined,
): boolean {
  if (!track) return false;
  return Boolean(track.dolby_audio || isDolbyAudio(track.audio_quality));
}
