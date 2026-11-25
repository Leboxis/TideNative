import { create } from "zustand";
import type {
  AppPreferences,
  AudioDeviceDescriptor,
  BootstrapState,
  CoverPalette,
  LastFmStatus,
  PlaybackState,
  StreamFormatInfo,
  TrackSummary,
  UiFontFamily,
} from "@/bindings";
import { extractCoverPaletteFromUrl, getCachedCoverPalette } from "@/lib/covers";
import { telemetryRuntime } from "@/lib/telemetry";

const DEFAULT_PALETTE: CoverPalette = {
  primary: "#111116",
  secondary: "#0A0B0F",
  accent: "#33F3FF",
  surface_tint: "#0B0D14",
  row_colors: ["#33F3FF", "#6366F1", "#EC4899", "#F59E0B"],
};

export const UI_FONT_STACKS: Record<UiFontFamily, string> = {
  Default: '"Inter", "Segoe UI Variable", "Segoe UI", system-ui, -apple-system, sans-serif',
  SeasonMix: '"Season Mix", "season-mix", "Inter", "Segoe UI Variable", system-ui, sans-serif',
  DmSans: '"DM Sans", "Inter", "Segoe UI Variable", system-ui, sans-serif',
  PlusJakartaSans: '"Plus Jakarta Sans", "Inter", "Segoe UI Variable", system-ui, sans-serif',
};

const FONT_STORAGE_KEY = "tidenative_ui_font";

export function applyUiFont(font: UiFontFamily | undefined | null): void {
  const resolved: UiFontFamily =
    font && font in UI_FONT_STACKS ? font : "Default";
  if (typeof document !== "undefined") {
    document.documentElement.dataset.uiFont = resolved;
    document.documentElement.style.setProperty("--font-sans", UI_FONT_STACKS[resolved]);
  }
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(FONT_STORAGE_KEY, resolved);
    }
  } catch {
    // Ignore storage access errors in restricted test environments.
  }
}

function readStoredUiFont(): UiFontFamily {
  try {
    if (typeof localStorage !== "undefined") {
      const raw = localStorage.getItem(FONT_STORAGE_KEY) as UiFontFamily | null;
      if (raw && raw in UI_FONT_STACKS) {
        return raw;
      }
    }
  } catch {
    // Ignore storage access errors.
  }
  return "Default";
}

const initialFont = readStoredUiFont();
applyUiFont(initialFont);

const DEFAULT_PREFS: AppPreferences = {
  device_id: null,
  exclusive_mode: true,
  fallback_to_shared: true,
  bit_perfect_lock: false,
  audio_quality: "HiResLossless",
  volume: 1.0,
  normalize_volume: true,
  replay_gain_mode: "Track",
  allow_explicit: true,
  allow_ai_content: false,
  target_fps: 120,
  shuffle_enabled: false,
  repeat_mode: "Off",
  gapless_playback: true,
  autoplay_radio: true,
  minimize_to_tray: false,
  lastfm_enabled: false,
  ui_font: initialFont,
};

interface PlaybackStore {
  authenticated: boolean;
  userId: number | null;
  countryCode: string | null;
  playbackState: PlaybackState;
  activeFormat: StreamFormatInfo | null;
  currentTrack: TrackSummary | null;
  currentPalette: CoverPalette;
  queue: TrackSummary[];
  queueIndex: number | null;
  durationMs: number;
  devices: AudioDeviceDescriptor[];
  preferences: AppPreferences;
  lastfm: LastFmStatus;
  hydrate: (boot: BootstrapState) => void;
  setPlaybackState: (state: PlaybackState) => void;
  setTrackChanged: (
    track: TrackSummary | null,
    palette: CoverPalette,
    queueIndex: number | null,
    durationMs: number,
  ) => void;
  setCurrentPalette: (palette: CoverPalette) => void;
  setQueue: (queue: TrackSummary[], queueIndex: number | null) => void;
  setActiveFormat: (format: StreamFormatInfo | null) => void;
  setDevices: (devices: AudioDeviceDescriptor[]) => void;
  setPreferences: (prefs: AppPreferences) => void;
  setLastFm: (status: LastFmStatus) => void;
  setAuth: (authenticated: boolean, userId: number | null, countryCode: string | null) => void;
}

export const usePlaybackStore = create<PlaybackStore>((set, get) => ({
  authenticated: false,
  userId: null,
  countryCode: null,
  playbackState: "Stopped",
  activeFormat: null,
  currentTrack: null,
  currentPalette: DEFAULT_PALETTE,
  queue: [],
  queueIndex: null,
  durationMs: 0,
  devices: [],
  preferences: DEFAULT_PREFS,
  lastfm: { connected: false, username: null, enabled: false },

  hydrate: (boot) => {
    telemetryRuntime.seedPosition(boot.position_ms, boot.duration_ms);
    applyUiFont(boot.preferences.ui_font);
    const cachedPalette = getCachedCoverPalette(boot.current_track?.cover_id);
    set({
      authenticated: boot.authenticated,
      userId: boot.user_id,
      countryCode: boot.country_code,
      playbackState: boot.playback_state,
      activeFormat: boot.active_format,
      currentTrack: boot.current_track,
      currentPalette: cachedPalette ?? boot.current_palette,
      queue: boot.queue,
      queueIndex: boot.queue_index,
      durationMs: boot.duration_ms,
      devices: boot.devices,
      preferences: boot.preferences,
      lastfm: boot.lastfm,
    });
    if (boot.current_track?.cover_id) {
      const trackId = boot.current_track.id;
      void extractCoverPaletteFromUrl(boot.current_track.cover_id).then((extracted) => {
        if (extracted && get().currentTrack?.id === trackId) {
          set({ currentPalette: extracted });
        }
      });
    }
  },
  setPlaybackState: (playbackState) => set({ playbackState }),
  setTrackChanged: (currentTrack, currentPalette, queueIndex, durationMs) => {
    const prevTrackId = get().currentTrack?.id ?? null;
    const prevQueueIndex = get().queueIndex;
    const nextTrackId = currentTrack?.id ?? null;
    if (prevTrackId !== nextTrackId || prevQueueIndex !== queueIndex) {
      telemetryRuntime.seedPosition(0, durationMs);
    } else if (durationMs > 0 && telemetryRuntime.durationMs !== durationMs) {
      telemetryRuntime.seedPosition(telemetryRuntime.positionMs, durationMs);
    }
    const cachedPalette = getCachedCoverPalette(currentTrack?.cover_id);
    set({
      currentTrack,
      currentPalette: cachedPalette ?? currentPalette,
      queueIndex,
      durationMs,
    });
    if (currentTrack?.cover_id) {
      const trackId = currentTrack.id;
      void extractCoverPaletteFromUrl(currentTrack.cover_id).then((extracted) => {
        if (extracted && get().currentTrack?.id === trackId) {
          set({ currentPalette: extracted });
        }
      });
    }
  },
  setCurrentPalette: (currentPalette) => set({ currentPalette }),
  setQueue: (queue, queueIndex) => set({ queue, queueIndex }),
  setActiveFormat: (activeFormat) => set({ activeFormat }),
  setDevices: (devices) => set({ devices }),
  setPreferences: (preferences) => {
    applyUiFont(preferences.ui_font);
    set({ preferences });
  },
  setLastFm: (lastfm) => set({ lastfm }),
  setAuth: (authenticated, userId, countryCode) => set({ authenticated, userId, countryCode }),
}));
