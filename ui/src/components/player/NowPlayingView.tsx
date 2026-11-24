import { ChevronDown, Disc3, Minimize2, Pause, Play, SkipBack, SkipForward } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { type LyricsLine, type TrackCredits, type TrackLyrics, commands } from "@/bindings";
import { AmbientBackdrop } from "@/components/layout/AmbientBackdrop";
import { CoverImage } from "@/components/media/CoverImage";
import { MatrixOrbCanvas } from "@/components/player/MatrixOrbCanvas";
import { Scrubber } from "@/components/player/Scrubber";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { telemetryRuntime } from "@/lib/telemetry";
import { qualityTierFromApi } from "@/lib/utils";
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
  const currentTrack = usePlaybackStore((s) => s.currentTrack);
  const palette = usePlaybackStore((s) => s.currentPalette);
  const playbackState = usePlaybackStore((s) => s.playbackState);
  const activeFormat = usePlaybackStore((s) => s.activeFormat);
  const durationMs = usePlaybackStore((s) => s.durationMs);

  const [lyrics, setLyrics] = useState<TrackLyrics | null>(null);
  const [credits, setCredits] = useState<TrackCredits | null>(null);
  const [activeLineIdx, setActiveLineIdx] = useState(-1);
  const activeLineRef = useRef<HTMLButtonElement | null>(null);
  const lastIdxRef = useRef(-1);

  const trackId = currentTrack?.id ?? null;

  useEffect(() => {
    if (!open || trackId === null) return;
    let alive = true;
    setLyrics(null);
    setCredits(null);
    lastIdxRef.current = -1;
    setActiveLineIdx(-1);

    void commands.getLyrics(trackId).then((res) => {
      if (alive && res.status === "ok") setLyrics(res.data);
    });
    void commands.getCredits(trackId).then((res) => {
      if (alive && res.status === "ok") setCredits(res.data);
    });
    return () => {
      alive = false;
    };
  }, [open, trackId]);

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
    activeLineRef.current?.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [activeLineIdx]);

  const isPlaying = playbackState === "Playing";
  const tier = qualityTierFromApi(currentTrack?.audio_quality);

  return (
    <AnimatePresence>
      {open && currentTrack && (
        <motion.div
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
              <button
                type="button"
                onClick={() => setOpen(false)}
                className="inline-flex items-center gap-2 rounded-full border border-white/10 bg-white/[0.06] px-4 py-1.5 text-xs font-medium text-white/80 hover:bg-white/12 hover:text-white transition-colors cursor-pointer"
              >
                <ChevronDown className="h-4 w-4" />
                Réduire
              </button>
              <button
                type="button"
                onClick={() => setCompactMode(true)}
                className="inline-flex items-center gap-2 rounded-full border border-white/10 bg-white/[0.06] px-4 py-1.5 text-xs font-medium text-white/80 hover:bg-white/12 hover:text-white transition-colors cursor-pointer"
              >
                <Minimize2 className="h-3.5 w-3.5" />
                Mode compact
              </button>
            </div>

            <div className="flex items-center gap-2">
              <Badge variant={tier === "MAX" ? "max" : tier === "HIGH" ? "high" : "low"}>
                {tier}
              </Badge>
              {activeFormat?.is_bit_perfect && (
                <Badge variant="bitperfect">Bit-Perfect</Badge>
              )}
              {activeFormat && (
                <span className="text-xs text-white/50 tabular-nums">
                  {activeFormat.source_pcm.bits_per_sample}-bit /{" "}
                  {(activeFormat.source_pcm.sample_rate / 1000).toFixed(1)} kHz
                </span>
              )}
            </div>
          </div>

          {/* Main 2-column immersive layout */}
          <div className="relative z-10 mx-auto grid w-full max-w-6xl flex-1 min-h-0 grid-cols-1 gap-10 px-8 pb-10 lg:grid-cols-[420px_1fr]">
            {/* Left: Cover + Orb + Transport */}
            <div className="flex flex-col items-center justify-center gap-6">
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
                <h1 className="truncate text-2xl font-bold tracking-tight text-white">
                  {currentTrack.title}
                </h1>
                <p className="truncate text-sm text-white/60">
                  {currentTrack.artist_name} — {currentTrack.album_title}
                </p>
              </div>

              <div className="w-full max-w-sm space-y-4">
                <Scrubber
                  durationMs={durationMs}
                  accentColor={palette.accent}
                  onSeek={(ms) => void commands.seekMs(ms)}
                />
                <div className="flex items-center justify-center gap-5">
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
                </div>
              </div>
            </div>

            {/* Right: Synced Lyrics & Credits */}
            <div className="flex min-h-0 flex-col rounded-3xl border border-white/[0.08] bg-black/30 p-6 backdrop-blur-md">
              <Tabs defaultValue="lyrics" className="flex flex-1 min-h-0 flex-col">
                <TabsList className="self-start">
                  <TabsTrigger value="lyrics">Paroles</TabsTrigger>
                  <TabsTrigger value="credits">Crédits</TabsTrigger>
                </TabsList>

                <TabsContent
                  value="lyrics"
                  className="flex-1 min-h-0 overflow-y-auto pr-2 space-y-3"
                >
                  {lyrics && lyrics.synced_lines.length > 0 ? (
                    <div className="py-12 space-y-4">
                      {lyrics.synced_lines.map((line, i) => {
                        const active = i === activeLineIdx;
                        return (
                          <button
                            key={`${line.time_ms}-${i}`}
                            ref={active ? activeLineRef : undefined}
                            type="button"
                            onClick={() => void commands.seekMs(line.time_ms)}
                            className={`block w-full text-left text-2xl font-semibold leading-snug transition-[opacity,transform] duration-200 cursor-pointer ${
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
                    <pre className="whitespace-pre-wrap font-sans text-base leading-relaxed text-white/75">
                      {lyrics.plain_text}
                    </pre>
                  ) : (
                    <div className="flex h-full flex-col items-center justify-center gap-2 text-white/40">
                      <Disc3 className="h-8 w-8 opacity-40" />
                      <p className="text-sm">Paroles indisponibles pour ce titre.</p>
                    </div>
                  )}
                </TabsContent>

                <TabsContent value="credits" className="flex-1 min-h-0 overflow-y-auto space-y-4 pr-2">
                  {credits && credits.groups.length > 0 ? (
                    credits.groups.map((g) => (
                      <div key={g.role} className="space-y-1">
                        <p className="text-[11px] font-semibold uppercase tracking-wider text-white/45">
                          {g.role}
                        </p>
                        <p className="text-sm text-white/90">{g.contributors.join(", ")}</p>
                      </div>
                    ))
                  ) : (
                    <p className="py-12 text-center text-sm text-white/40">
                      Aucun crédit disponible pour ce titre.
                    </p>
                  )}
                </TabsContent>
              </Tabs>
            </div>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
