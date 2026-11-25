import {
  ChevronDown,
  Disc3,
  GripVertical,
  Heart,
  LocateFixed,
  Lock,
  Minimize2,
  Pause,
  Play,
  Repeat,
  Repeat1,
  Shuffle,
  SkipBack,
  SkipForward,
  Trash2,
  Volume2,
  VolumeX,
} from "lucide-react";
import { AnimatePresence, Reorder, motion } from "motion/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import {
  type LyricsLine,
  type TrackCredits,
  type TrackLyrics,
  type TrackSummary,
  commands,
} from "@/bindings";
import { AmbientBackdrop } from "@/components/layout/AmbientBackdrop";
import { CoverImage } from "@/components/media/CoverImage";
import { DolbyAudioLogo } from "@/components/media/DolbyAudioLogo";
import { MatrixOrbCanvas } from "@/components/player/MatrixOrbCanvas";
import { Scrubber } from "@/components/player/Scrubber";
import { Slider } from "@/components/ui/slider";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ActionTooltip } from "@/components/ui/tooltip";
import { telemetryRuntime } from "@/lib/telemetry";
import { formatSecs, hasDolbyAudio } from "@/lib/utils";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";

export function activeLyricsIndex(lines: LyricsLine[], positionMs: number): number {
  if (lines.length === 0) return -1;
  let lo = 0;
  let hi = lines.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (lines[mid].time_ms <= positionMs) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  return lo - 1;
}

export function NowPlayingView() {
  const open = useUiStore((s) => s.nowPlayingOpen);
  const setOpen = useUiStore((s) => s.setNowPlayingOpen);
  const setCompactMode = useUiStore((s) => s.setCompactMode);
  const navigate = useUiStore((s) => s.navigate);

  const currentTrack = usePlaybackStore((s) => s.currentTrack);
  const palette = usePlaybackStore((s) => s.currentPalette);
  const playbackState = usePlaybackStore((s) => s.playbackState);
  const durationMs = usePlaybackStore((s) => s.durationMs);
  const preferences = usePlaybackStore((s) => s.preferences);
  const queue = usePlaybackStore((s) => s.queue);
  const queueIndex = usePlaybackStore((s) => s.queueIndex);

  const isFav = useLibraryStore((s) => (currentTrack ? s.favoriteIds.has(currentTrack.id) : false));
  const toggleFavoriteLocal = useLibraryStore((s) => s.toggleFavoriteTrackLocal);

  const [lyrics, setLyrics] = useState<TrackLyrics | null>(null);
  const [credits, setCredits] = useState<TrackCredits | null>(null);
  const [activeLineIdx, setActiveLineIdx] = useState(-1);
  const [lyricsAutoScrollPaused, setLyricsAutoScrollPaused] = useState(false);
  const activeLineRef = useRef<HTMLButtonElement | null>(null);
  const lastIdxRef = useRef(-1);
  const resumeAutoScrollTimerRef = useRef<number | null>(null);
  const isProgrammaticScrollRef = useRef(false);
  const programmaticClearTimerRef = useRef<number | null>(null);

  const trackId = currentTrack?.id ?? null;
  const hasSyncedLyrics = Boolean(lyrics && lyrics.synced_lines.length > 0);

  const clearLyricsTimers = useCallback(() => {
    if (resumeAutoScrollTimerRef.current !== null) {
      window.clearTimeout(resumeAutoScrollTimerRef.current);
      resumeAutoScrollTimerRef.current = null;
    }
    if (programmaticClearTimerRef.current !== null) {
      window.clearTimeout(programmaticClearTimerRef.current);
      programmaticClearTimerRef.current = null;
    }
  }, []);

  const scrollToActiveLine = useCallback((instant = false) => {
    const el = activeLineRef.current;
    if (!el || typeof el.scrollIntoView !== "function") return;
    isProgrammaticScrollRef.current = true;
    if (programmaticClearTimerRef.current !== null) {
      window.clearTimeout(programmaticClearTimerRef.current);
    }
    const prefersReduced =
      typeof window !== "undefined" &&
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    el.scrollIntoView({
      behavior: instant || prefersReduced ? "auto" : "smooth",
      block: "center",
    });
    programmaticClearTimerRef.current = window.setTimeout(() => {
      isProgrammaticScrollRef.current = false;
      programmaticClearTimerRef.current = null;
    }, 650);
  }, []);

  const pauseLyricsAutoScroll = useCallback(() => {
    if (!hasSyncedLyrics) return;
    setLyricsAutoScrollPaused(true);
    if (resumeAutoScrollTimerRef.current !== null) {
      window.clearTimeout(resumeAutoScrollTimerRef.current);
    }
    resumeAutoScrollTimerRef.current = window.setTimeout(() => {
      resumeAutoScrollTimerRef.current = null;
      setLyricsAutoScrollPaused(false);
    }, 8000);
  }, [hasSyncedLyrics]);

  const resumeLyricsAutoScroll = useCallback(() => {
    if (resumeAutoScrollTimerRef.current !== null) {
      window.clearTimeout(resumeAutoScrollTimerRef.current);
      resumeAutoScrollTimerRef.current = null;
    }
    setLyricsAutoScrollPaused(false);
  }, []);

  useEffect(() => clearLyricsTimers, [clearLyricsTimers]);

  useEffect(() => {
    if (!open || trackId === null) return;
    let alive = true;
    setLyrics(null);
    setCredits(null);
    lastIdxRef.current = -1;
    setActiveLineIdx(-1);
    setLyricsAutoScrollPaused(false);
    clearLyricsTimers();

    void Promise.all([commands.getLyrics(trackId), commands.getCredits(trackId)]).then(
      ([lyricsRes, creditsRes]) => {
        if (!alive) return;
        if (lyricsRes.status === "ok") setLyrics(lyricsRes.data);
        if (creditsRes.status === "ok") setCredits(creditsRes.data);
      },
    );
    return () => {
      alive = false;
    };
  }, [open, trackId, clearLyricsTimers]);

  useEffect(() => {
    if (!open || !lyrics || lyrics.synced_lines.length === 0) return;
    const synced = lyrics.synced_lines;
    const initial = activeLyricsIndex(synced, telemetryRuntime.positionMs);
    lastIdxRef.current = initial;
    setActiveLineIdx(initial);

    return telemetryRuntime.subscribe((meta) => {
      const idx = activeLyricsIndex(synced, meta.positionMs);
      if (idx !== lastIdxRef.current) {
        lastIdxRef.current = idx;
        setActiveLineIdx(idx);
      }
    });
  }, [open, lyrics]);

  useEffect(() => {
    if (lyricsAutoScrollPaused || activeLineIdx < 0) return;
    scrollToActiveLine(false);
  }, [activeLineIdx, lyricsAutoScrollPaused, scrollToActiveLine]);

  const isPlaying = playbackState === "Playing";

  const upcomingStart = queueIndex !== null ? queueIndex + 1 : 0;
  const upcomingTracks = useMemo(() => queue.slice(upcomingStart), [queue, upcomingStart]);
  const [upcomingRows, setUpcomingRows] = useState<{ uid: string; track: TrackSummary }[]>(() =>
    upcomingTracks.map((track, idx) => ({ uid: `${track.id}-${idx}`, track })),
  );
  const queueDragStartRef = useRef<{ uid: string; index: number } | null>(null);
  const queueDraggingRef = useRef(false);

  useEffect(() => {
    if (!queueDraggingRef.current) {
      const seen = new Map<number, number>();
      setUpcomingRows(
        upcomingTracks.map((track) => {
          const count = seen.get(track.id) ?? 0;
          seen.set(track.id, count + 1);
          return { uid: `${track.id}#${count}`, track };
        }),
      );
    }
  }, [upcomingTracks]);

  const handleToggleFavorite = async () => {
    if (!currentTrack) return;
    const res = await commands.toggleFavoriteTrack(currentTrack);
    if (res.status === "ok") {
      toggleFavoriteLocal(currentTrack, res.data);
      toast.success(res.data ? "Ajouté aux favoris" : "Retiré des favoris");
    } else {
      toast.error(res.error);
    }
  };

  const handleNavigateArtist = () => {
    if (!currentTrack?.artist_id) return;
    setOpen(false);
    navigate({ name: "artist", artistId: currentTrack.artist_id });
  };

  const handleNavigateAlbum = () => {
    if (!currentTrack?.album_id) return;
    setOpen(false);
    navigate({ name: "album", albumId: currentTrack.album_id });
  };

  return (
    <AnimatePresence>
      {open && currentTrack ? (
        <motion.div
          role="dialog"
          aria-modal="true"
          aria-label="En cours de lecture"
          initial={{ opacity: 0, y: 24 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 24 }}
          transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
          className="fixed inset-0 z-50 flex flex-col overflow-hidden bg-[#040406] text-white"
        >
          <AmbientBackdrop palette={palette} intensity="immersive" />

          {/* Header */}
          <div
            data-tauri-drag-region
            className="relative z-10 flex items-center justify-between px-8 py-5 select-none"
          >
            <div className="flex items-center gap-2">
              <ActionTooltip label="Quitter le plein écran" shortcut="Échap" side="bottom">
                <button
                  type="button"
                  onClick={() => setOpen(false)}
                  className="inline-flex items-center gap-2 rounded-full border border-white/10 bg-white/[0.06] px-4 py-1.5 text-xs font-medium text-white/80 hover:bg-white/12 hover:text-white transition-colors cursor-pointer"
                >
                  <ChevronDown className="h-4 w-4" />
                  Réduire
                </button>
              </ActionTooltip>
              <ActionTooltip label="Mode compact" shortcut="C" side="bottom">
                <button
                  type="button"
                  onClick={() => setCompactMode(true)}
                  className="inline-flex items-center gap-2 rounded-full border border-white/10 bg-white/[0.06] px-4 py-1.5 text-xs font-medium text-white/80 hover:bg-white/12 hover:text-white transition-colors cursor-pointer"
                >
                  <Minimize2 className="h-3.5 w-3.5" />
                  Mode compact
                </button>
              </ActionTooltip>
            </div>
          </div>

          {/* Main 2-column immersive layout */}
          <div className="relative z-10 mx-auto grid w-full max-w-6xl flex-1 min-h-0 grid-cols-1 gap-10 px-8 pb-10 lg:grid-cols-[420px_1fr]">
            {/* Left: Cover + Orb + Transport */}
            <div className="flex flex-col items-center justify-center gap-5">
              <div className="relative">
                <CoverImage
                  coverId={currentTrack.cover_id}
                  alt={currentTrack.title}
                  size={1280}
                  className="h-80 w-80 shadow-[0_30px_80px_rgba(0,0,0,0.75)] border border-white/10"
                  rounded="2xl"
                />
                <div className="absolute -bottom-4 -right-4 rounded-full border border-white/12 bg-[#0a0b0f]/90 p-2 backdrop-blur-md shadow-xl">
                  <MatrixOrbCanvas active={isPlaying} accentColor={palette.accent} size={44} />
                </div>
              </div>

              <div className="w-full max-w-sm text-center space-y-1">
                <div className="flex items-center justify-center gap-2">
                  <h1 className="truncate text-2xl font-bold tracking-tight text-white">
                    {currentTrack.title}
                  </h1>
                  {hasDolbyAudio(currentTrack) && <DolbyAudioLogo size="md" />}
                </div>
                <div className="flex items-center justify-center gap-1.5 text-sm text-white/60">
                  {currentTrack.artist_id ? (
                    <button
                      type="button"
                      onClick={handleNavigateArtist}
                      className="truncate hover:text-white hover:underline cursor-pointer"
                    >
                      {currentTrack.artist_name}
                    </button>
                  ) : (
                    <span className="truncate">{currentTrack.artist_name}</span>
                  )}
                  {currentTrack.album_title ? (
                    <>
                      <span className="text-white/30">—</span>
                      {currentTrack.album_id ? (
                        <button
                          type="button"
                          onClick={handleNavigateAlbum}
                          className="truncate hover:text-white hover:underline cursor-pointer"
                        >
                          {currentTrack.album_title}
                        </button>
                      ) : (
                        <span className="truncate">{currentTrack.album_title}</span>
                      )}
                    </>
                  ) : null}
                </div>
              </div>

              <div className="w-full max-w-sm space-y-4">
                <Scrubber
                  durationMs={durationMs}
                  accentColor={palette.accent}
                  onSeek={(ms) => void commands.seekMs(ms)}
                />

                <div className="flex items-center justify-center gap-3">
                  <button
                    type="button"
                    aria-label="Aléatoire"
                    aria-pressed={preferences.shuffle_enabled}
                    onClick={() => void commands.setShuffle(!preferences.shuffle_enabled)}
                    className={`flex h-9 w-9 items-center justify-center rounded-full transition-colors cursor-pointer ${
                      preferences.shuffle_enabled
                        ? "bg-[#33F3FF]/15 text-[#33F3FF]"
                        : "text-white/60 hover:bg-white/10 hover:text-white"
                    }`}
                  >
                    <Shuffle className="h-4 w-4" />
                  </button>
                  <button
                    type="button"
                    aria-label="Précédent"
                    onClick={() => void commands.prevTrack()}
                    className="flex h-10 w-10 items-center justify-center rounded-full text-white/80 hover:bg-white/10 hover:text-white cursor-pointer"
                  >
                    <SkipBack className="h-5 w-5 fill-current" />
                  </button>
                  <button
                    type="button"
                    aria-label={isPlaying ? "Pause" : "Lecture"}
                    onClick={() => void commands.togglePlayPause()}
                    className="flex h-12 w-12 items-center justify-center rounded-full bg-white text-[#050507] hover:bg-[#e8e8ec] active:scale-95 transition-transform cursor-pointer"
                  >
                    {isPlaying ? (
                      <Pause className="h-5 w-5 fill-current" />
                    ) : (
                      <Play className="h-5 w-5 fill-current translate-x-[1px]" />
                    )}
                  </button>
                  <button
                    type="button"
                    aria-label="Suivant"
                    onClick={() => void commands.nextTrack()}
                    className="flex h-10 w-10 items-center justify-center rounded-full text-white/80 hover:bg-white/10 hover:text-white cursor-pointer"
                  >
                    <SkipForward className="h-5 w-5 fill-current" />
                  </button>
                  {(() => {
                    const repeatMode = preferences.repeat_mode ?? "Off";
                    const repeatTitle =
                      repeatMode === "One"
                        ? "Répétition : titre en boucle"
                        : repeatMode === "All"
                          ? "Répétition : toute la file"
                          : "Répétition : désactivée";
                    return (
                      <ActionTooltip label={repeatTitle}>
                        <button
                          type="button"
                          aria-label="Répétition"
                          aria-pressed={repeatMode !== "Off"}
                          data-repeat-mode={repeatMode}
                          onClick={() => void commands.cycleRepeat()}
                          className={`flex h-9 w-9 items-center justify-center rounded-full transition-colors cursor-pointer ${
                            repeatMode !== "Off"
                              ? "bg-[#33F3FF]/15 text-[#33F3FF]"
                              : "text-white/60 hover:bg-white/10 hover:text-white"
                          }`}
                        >
                          {repeatMode === "One" ? (
                            <Repeat1 className="h-4 w-4" />
                          ) : (
                            <Repeat className="h-4 w-4" />
                          )}
                        </button>
                      </ActionTooltip>
                    );
                  })()}
                </div>

                {/* Favorite + Volume */}
                <div className="flex items-center justify-between gap-4 pt-1">
                  <button
                    type="button"
                    aria-label={isFav ? "Retirer des favoris" : "Ajouter aux favoris"}
                    onClick={() => void handleToggleFavorite()}
                    className={`flex h-9 w-9 items-center justify-center rounded-full border border-white/10 bg-white/[0.04] transition-colors cursor-pointer ${
                      isFav ? "text-[#FF2D55] border-[#FF2D55]/30" : "text-white/65 hover:text-white"
                    }`}
                  >
                    <Heart className="h-4 w-4" fill={isFav ? "currentColor" : "none"} />
                  </button>

                  <div className="flex flex-1 items-center gap-2.5 max-w-[180px]">
                    <button
                      type="button"
                      aria-label={preferences.volume <= 0.005 ? "Rétablir le son" : "Muet"}
                      onClick={() => void commands.toggleMute()}
                      className="text-white/60 hover:text-white transition-colors cursor-pointer"
                    >
                      {preferences.bit_perfect_lock ? (
                        <Lock className="h-3.5 w-3.5 text-[#30D158]" />
                      ) : preferences.volume <= 0.005 ? (
                        <VolumeX className="h-4 w-4" />
                      ) : (
                        <Volume2 className="h-4 w-4" />
                      )}
                    </button>
                    <Slider
                      aria-label="Volume plein écran"
                      min={0}
                      max={100}
                      step={1}
                      value={[
                        Math.round((preferences.bit_perfect_lock ? 1 : preferences.volume) * 100),
                      ]}
                      disabled={preferences.bit_perfect_lock}
                      onValueChange={([v]) => void commands.setVolume(v / 100)}
                    />
                  </div>
                </div>
              </div>
            </div>

            {/* Right: Synced Lyrics, Up Next & Credits */}
            <div className="flex min-h-0 flex-col rounded-3xl border border-white/[0.08] bg-black/30 p-6 backdrop-blur-md">
              <Tabs defaultValue="lyrics" className="flex flex-1 min-h-0 flex-col">
                <TabsList className="self-start">
                  <TabsTrigger value="lyrics">Paroles</TabsTrigger>
                  <TabsTrigger value="queue">À suivre ({queue.length})</TabsTrigger>
                  <TabsTrigger value="credits">Crédits</TabsTrigger>
                </TabsList>

                <TabsContent value="lyrics" className="relative flex-1 min-h-0">
                  <div
                    data-testid="lyrics-scroll-container"
                    onWheel={pauseLyricsAutoScroll}
                    onTouchMove={pauseLyricsAutoScroll}
                    onScroll={() => {
                      if (!isProgrammaticScrollRef.current && hasSyncedLyrics) {
                        pauseLyricsAutoScroll();
                      }
                    }}
                    className="lyrics-fade-mask h-full overflow-y-auto pr-2 space-y-3"
                  >
                    {lyrics && lyrics.synced_lines.length > 0 ? (
                      <div className="py-16 space-y-4">
                        {lyrics.synced_lines.map((line, i) => {
                          const active = i === activeLineIdx;
                          return (
                            <button
                              key={`${line.time_ms}-${i}`}
                              ref={active ? activeLineRef : undefined}
                              type="button"
                              onClick={() => {
                                resumeLyricsAutoScroll();
                                void commands.seekMs(line.time_ms);
                              }}
                              className={`block w-full text-left text-2xl font-semibold leading-snug transition-[opacity,transform,color] duration-200 cursor-pointer ${
                                active
                                  ? "text-white opacity-100 translate-x-1"
                                  : "text-white/35 opacity-60 hover:opacity-85"
                              }`}
                            >
                              {line.text}
                            </button>
                          );
                        })}
                      </div>
                    ) : lyrics?.plain_text ? (
                      <pre className="py-6 whitespace-pre-wrap font-sans text-base leading-relaxed text-white/75">
                        {lyrics.plain_text}
                      </pre>
                    ) : (
                      <div className="flex h-full flex-col items-center justify-center gap-2 text-white/60">
                        <Disc3 className="h-8 w-8 opacity-50" />
                        <p className="text-sm">Paroles indisponibles pour ce titre.</p>
                      </div>
                    )}
                  </div>

                  {lyricsAutoScrollPaused && hasSyncedLyrics && activeLineIdx >= 0 ? (
                    <div className="pointer-events-none absolute inset-x-0 bottom-3 flex justify-center">
                      <button
                        type="button"
                        onClick={resumeLyricsAutoScroll}
                        className="pointer-events-auto glass-surface inline-flex items-center gap-2 rounded-full px-3.5 py-1.5 text-xs font-medium text-white shadow-lg transition-[transform,border-color,background-color] duration-150 hover:border-white/25 hover:bg-white/[0.12] active:scale-95 cursor-pointer"
                      >
                        <LocateFixed className="h-3.5 w-3.5 text-[#33F3FF]" />
                        Revenir à la ligne en cours
                      </button>
                    </div>
                  ) : null}
                </TabsContent>

                <TabsContent
                  value="queue"
                  className="flex-1 min-h-0 overflow-y-auto pr-2 space-y-2"
                >
                  {queue.length === 0 ? (
                    <p className="py-12 text-center text-sm text-white/60">
                      La file d&apos;attente est vide.
                    </p>
                  ) : (
                    <>
                      {queue.slice(0, upcomingStart).map((track, idx) => {
                        const isCurrent = queueIndex === idx;
                        return (
                          <div
                            key={`${track.id}-${idx}`}
                            className={`group flex items-center gap-3 rounded-xl px-3 py-2 transition-colors ${
                              isCurrent
                                ? "bg-white/[0.10] border border-white/15"
                                : "hover:bg-white/[0.05] border border-transparent opacity-70"
                            }`}
                          >
                            <button
                              type="button"
                              onClick={() => void commands.playQueueIndex(idx)}
                              className="flex min-w-0 flex-1 items-center gap-3 text-left cursor-pointer"
                            >
                              <CoverImage
                                coverId={track.cover_id}
                                alt={track.title}
                                size={80}
                                className="h-10 w-10"
                                rounded="md"
                              />
                              <div className="min-w-0 flex-1">
                                <p
                                  className={`truncate text-sm font-medium ${
                                    isCurrent ? "text-[#33F3FF]" : "text-white"
                                  }`}
                                >
                                  {track.title}
                                </p>
                                <p className="truncate text-xs text-white/60">
                                  {track.artist_name}
                                </p>
                              </div>
                            </button>
                            <span className="text-xs text-white/60 tabular-nums">
                              {formatSecs(track.duration_secs)}
                            </span>
                            <button
                              type="button"
                              aria-label={`Retirer ${track.title} de la file`}
                              onClick={() => void commands.queueRemove(idx)}
                              className="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 flex h-7 w-7 items-center justify-center rounded-lg text-white/60 hover:bg-white/10 hover:text-[#ff453a] transition-all cursor-pointer"
                            >
                              <Trash2 className="h-3.5 w-3.5" />
                            </button>
                          </div>
                        );
                      })}

                      {upcomingRows.length > 0 ? (
                        <Reorder.Group
                          axis="y"
                          values={upcomingRows}
                          onReorder={setUpcomingRows}
                          className="space-y-1.5"
                        >
                          {upcomingRows.map((row, offset) => {
                            const absIdx = upcomingStart + offset;
                            const { track } = row;
                            return (
                              <Reorder.Item
                                key={row.uid}
                                value={row}
                                onDragStart={() => {
                                  queueDraggingRef.current = true;
                                  queueDragStartRef.current = { uid: row.uid, index: offset };
                                }}
                                onDragEnd={() => {
                                  queueDraggingRef.current = false;
                                  const start = queueDragStartRef.current;
                                  queueDragStartRef.current = null;
                                  if (!start) return;
                                  const endIdx = upcomingRows.findIndex((r) => r.uid === start.uid);
                                  if (endIdx >= 0 && endIdx !== start.index) {
                                    void commands.queueMove(
                                      upcomingStart + start.index,
                                      upcomingStart + endIdx,
                                    );
                                  }
                                }}
                                className="group flex items-center gap-2.5 rounded-xl px-2.5 py-2 hover:bg-white/[0.05] border border-transparent hover:border-white/10 transition-colors cursor-grab active:cursor-grabbing"
                              >
                                <GripVertical className="h-3.5 w-3.5 text-white/35 group-hover:text-white/65 shrink-0" />
                                <button
                                  type="button"
                                  onClick={() => void commands.playQueueIndex(absIdx)}
                                  className="flex min-w-0 flex-1 items-center gap-3 text-left cursor-pointer"
                                >
                                  <CoverImage
                                    coverId={track.cover_id}
                                    alt={track.title}
                                    size={80}
                                    className="h-10 w-10"
                                    rounded="md"
                                  />
                                  <div className="min-w-0 flex-1">
                                    <p className="truncate text-sm font-medium text-white">
                                      {track.title}
                                    </p>
                                    <p className="truncate text-xs text-white/60">
                                      {track.artist_name}
                                    </p>
                                  </div>
                                </button>
                                <span className="text-xs text-white/60 tabular-nums">
                                  {formatSecs(track.duration_secs)}
                                </span>
                                <button
                                  type="button"
                                  aria-label={`Retirer ${track.title} de la file`}
                                  onClick={() => void commands.queueRemove(absIdx)}
                                  className="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 flex h-7 w-7 items-center justify-center rounded-lg text-white/60 hover:bg-white/10 hover:text-[#ff453a] transition-all cursor-pointer"
                                >
                                  <Trash2 className="h-3.5 w-3.5" />
                                </button>
                              </Reorder.Item>
                            );
                          })}
                        </Reorder.Group>
                      ) : null}
                    </>
                  )}
                </TabsContent>

                <TabsContent
                  value="credits"
                  className="flex-1 min-h-0 overflow-y-auto space-y-4 pr-2"
                >
                  {credits && credits.groups.length > 0 ? (
                    credits.groups.map((g) => (
                      <div key={g.role} className="space-y-1">
                        <p className="text-[11px] font-semibold uppercase tracking-wider text-white/60">
                          {g.role}
                        </p>
                        <p className="text-sm text-white/90">{g.contributors.join(", ")}</p>
                      </div>
                    ))
                  ) : (
                    <p className="py-12 text-center text-sm text-white/60">
                      Aucun crédit disponible pour ce titre.
                    </p>
                  )}
                </TabsContent>
              </Tabs>
            </div>
          </div>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
