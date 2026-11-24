import {
  Heart,
  ListMusic,
  Lock,
  Maximize2,
  Minimize2,
  Pause,
  Play,
  Repeat,
  Repeat1,
  Shuffle,
  SkipBack,
  SkipForward,
  Volume2,
  VolumeX,
} from "lucide-react";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { MatrixOrbCanvas } from "@/components/player/MatrixOrbCanvas";
import { Scrubber } from "@/components/player/Scrubber";
import { Badge } from "@/components/ui/badge";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Slider } from "@/components/ui/slider";
import { coverUrl } from "@/lib/covers";
import { qualityTierFromApi } from "@/lib/utils";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";

export function PlayerBar() {
  const currentTrack = usePlaybackStore((s) => s.currentTrack);
  const currentPalette = usePlaybackStore((s) => s.currentPalette);
  const playbackState = usePlaybackStore((s) => s.playbackState);
  const activeFormat = usePlaybackStore((s) => s.activeFormat);
  const durationMs = usePlaybackStore((s) => s.durationMs);
  const preferences = usePlaybackStore((s) => s.preferences);
  const favoriteIds = useLibraryStore((s) => s.favoriteIds);
  const toggleFavoriteLocal = useLibraryStore((s) => s.toggleFavoriteTrackLocal);
  const navigate = useUiStore((s) => s.navigate);
  const setNowPlayingOpen = useUiStore((s) => s.setNowPlayingOpen);
  const queueOpen = useUiStore((s) => s.queueOpen);
  const setQueueOpen = useUiStore((s) => s.setQueueOpen);
  const setCompactMode = useUiStore((s) => s.setCompactMode);

  const isPlaying = playbackState === "Playing";
  const isBuffering = playbackState === "Buffering";
  const isFav = currentTrack ? favoriteIds.has(currentTrack.id) : false;
  const thumb = coverUrl(currentTrack?.cover_id, 160);
  const tier = qualityTierFromApi(currentTrack?.audio_quality);

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

  return (
    <footer className="relative z-30 px-4 pb-3 pt-1">
      <div className="glass-surface mx-auto flex h-[82px] max-w-[1440px] items-center justify-between gap-4 rounded-2xl px-4">
        {/* Left: Cover + Track Info + Favorite */}
        <div className="flex min-w-0 w-[30%] items-center gap-3">
          <button
            type="button"
            onClick={() => currentTrack && setNowPlayingOpen(true)}
            className="group relative h-12 w-12 shrink-0 overflow-hidden rounded-xl border border-white/10 bg-[#111116] cursor-pointer"
            title="Ouvrir En cours de lecture"
          >
            {thumb ? (
              <img src={thumb} alt={currentTrack?.title ?? ""} className="h-full w-full object-cover" />
            ) : (
              <div className="flex h-full w-full items-center justify-center">
                <MatrixOrbCanvas active={isPlaying} accentColor={currentPalette.accent} size={32} />
              </div>
            )}
            {currentTrack && (
              <span className="absolute inset-0 flex items-center justify-center bg-black/50 opacity-0 group-hover:opacity-100 transition-opacity duration-150">
                <Maximize2 className="h-4 w-4 text-white" />
              </span>
            )}
          </button>

          {currentTrack ? (
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => setNowPlayingOpen(true)}
                  className="truncate text-sm font-semibold text-white hover:underline cursor-pointer"
                >
                  {currentTrack.title}
                </button>
                <Badge variant={tier === "MAX" ? "max" : tier === "HIGH" ? "high" : "low"}>
                  {tier}
                </Badge>
              </div>
              <div className="flex items-center gap-1.5 text-xs text-white/55">
                <button
                  type="button"
                  onClick={() =>
                    currentTrack.artist_id &&
                    navigate({ name: "artist", artistId: currentTrack.artist_id })
                  }
                  className="truncate hover:text-white cursor-pointer"
                >
                  {currentTrack.artist_name}
                </button>
                {currentTrack.album_title && (
                  <>
                    <span className="text-white/25">•</span>
                    <button
                      type="button"
                      onClick={() =>
                        currentTrack.album_id &&
                        navigate({ name: "album", albumId: currentTrack.album_id })
                      }
                      className="truncate hover:text-white cursor-pointer"
                    >
                      {currentTrack.album_title}
                    </button>
                  </>
                )}
              </div>
            </div>
          ) : (
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium text-white/60">Prêt pour l&apos;écoute</p>
              <p className="text-xs text-white/35">WASAPI Bit-Perfect Hi-Res</p>
            </div>
          )}

          {currentTrack && (
            <button
              type="button"
              aria-label={isFav ? "Retirer des favoris" : "Ajouter aux favoris"}
              onClick={handleToggleFavorite}
              className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-lg transition-colors cursor-pointer ${
                isFav ? "text-[#FF2D55]" : "text-white/45 hover:text-white"
              }`}
            >
              <Heart className="h-4 w-4" fill={isFav ? "currentColor" : "none"} />
            </button>
          )}
        </div>

        {/* Center: Transport + Zero-Re-render Scrubber */}
        <div className="flex flex-1 max-w-[540px] flex-col items-center gap-1.5">
          <div className="flex items-center gap-3">
            <button
              type="button"
              aria-label="Aléatoire"
              onClick={() => void commands.setShuffle(!preferences.shuffle_enabled)}
              className={`flex h-7 w-7 items-center justify-center rounded-full transition-colors cursor-pointer ${
                preferences.shuffle_enabled
                  ? "text-[#33F3FF] bg-[#33F3FF]/10"
                  : "text-white/50 hover:text-white"
              }`}
            >
              <Shuffle className="h-3.5 w-3.5" />
            </button>

            <button
              type="button"
              aria-label="Piste précédente"
              onClick={() => void commands.prevTrack()}
              className="flex h-8 w-8 items-center justify-center rounded-full text-white/80 hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
            >
              <SkipBack className="h-4 w-4 fill-current" />
            </button>

            <button
              type="button"
              aria-label={isPlaying ? "Pause" : "Lecture"}
              onClick={() => void commands.togglePlayPause()}
              className="flex h-9 w-9 items-center justify-center rounded-full bg-white text-[#050507] hover:bg-[#e8e8ec] active:scale-95 transition-transform duration-150 cursor-pointer"
            >
              {isBuffering ? (
                <span className="h-4 w-4 animate-spin rounded-full border-2 border-[#050507]/30 border-t-[#050507]" />
              ) : isPlaying ? (
                <Pause className="h-4 w-4 fill-current" />
              ) : (
                <Play className="h-4 w-4 fill-current translate-x-[1px]" />
              )}
            </button>

            <button
              type="button"
              aria-label="Piste suivante"
              onClick={() => void commands.nextTrack()}
              className="flex h-8 w-8 items-center justify-center rounded-full text-white/80 hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
            >
              <SkipForward className="h-4 w-4 fill-current" />
            </button>

            <button
              type="button"
              aria-label="Répétition"
              onClick={() => void commands.cycleRepeat()}
              className={`flex h-7 w-7 items-center justify-center rounded-full transition-colors cursor-pointer ${
                preferences.repeat_mode !== "Off"
                  ? "text-[#33F3FF] bg-[#33F3FF]/10"
                  : "text-white/50 hover:text-white"
              }`}
            >
              {preferences.repeat_mode === "One" ? (
                <Repeat1 className="h-3.5 w-3.5" />
              ) : (
                <Repeat className="h-3.5 w-3.5" />
              )}
            </button>
          </div>

          <Scrubber
            durationMs={durationMs}
            accentColor={currentPalette.accent}
            onSeek={(ms) => void commands.seekMs(ms)}
            compact
          />
        </div>

        {/* Right: MatrixOrb + Signal Path Popover + Volume + Queue */}
        <div className="flex w-[30%] items-center justify-end gap-2.5">
          <MatrixOrbCanvas
            active={isPlaying}
            accentColor={currentPalette.accent}
            size={30}
            className="hidden lg:block shrink-0"
          />

          <Popover>
            <PopoverTrigger asChild>
              <button
                type="button"
                className="hidden xl:inline-flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1 text-[11px] font-medium text-white/75 hover:border-white/20 hover:text-white transition-colors cursor-pointer tabular-nums"
              >
                <span
                  className={`h-1.5 w-1.5 rounded-full ${
                    activeFormat?.is_bit_perfect ? "bg-[#30D158]" : "bg-[#33F3FF]"
                  }`}
                />
                {activeFormat
                  ? `${activeFormat.codec} ${activeFormat.source_pcm.bits_per_sample}b/${Math.round(activeFormat.source_pcm.sample_rate / 1000)}k`
                  : preferences.exclusive_mode
                    ? "WASAPI Excl."
                    : "WASAPI"}
              </button>
            </PopoverTrigger>
            <PopoverContent align="end" className="w-80">
              <div className="space-y-2.5">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-semibold text-white">Chaîne Audio WASAPI</span>
                  {activeFormat?.is_bit_perfect && <Badge variant="bitperfect">Bit-Perfect</Badge>}
                </div>
                {activeFormat ? (
                  <dl className="grid grid-cols-2 gap-y-1.5 text-xs">
                    <dt className="text-white/45">Codec</dt>
                    <dd className="text-right font-medium text-white">{activeFormat.codec}</dd>
                    <dt className="text-white/45">Source PCM</dt>
                    <dd className="text-right font-medium text-white tabular-nums">
                      {activeFormat.source_pcm.bits_per_sample}-bit /{" "}
                      {(activeFormat.source_pcm.sample_rate / 1000).toFixed(1)} kHz
                    </dd>
                    <dt className="text-white/45">Conteneur DAC</dt>
                    <dd className="text-right font-medium text-white tabular-nums">
                      {activeFormat.dac_container_bits}-bit ({activeFormat.dac_valid_bits}b valides)
                    </dd>
                    <dt className="text-white/45">Mode</dt>
                    <dd className="text-right font-medium text-white">
                      {activeFormat.is_exclusive ? "WASAPI Exclusif" : "WASAPI Partagé"}
                    </dd>
                    <dt className="text-white/45">Sortie</dt>
                    <dd className="text-right truncate font-medium text-white">
                      {activeFormat.device_name}
                    </dd>
                  </dl>
                ) : (
                  <p className="text-xs text-white/50">
                    Lancez un titre pour inspecter le format matériel négocié avec votre DAC.
                  </p>
                )}
              </div>
            </PopoverContent>
          </Popover>

          <div className="flex items-center gap-2 w-28">
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
              aria-label="Volume"
              min={0}
              max={100}
              step={1}
              value={[Math.round((preferences.bit_perfect_lock ? 1 : preferences.volume) * 100)]}
              disabled={preferences.bit_perfect_lock}
              onValueChange={([v]) => void commands.setVolume(v / 100)}
            />
          </div>

          <button
            type="button"
            aria-label="File d'attente"
            onClick={() => setQueueOpen(!queueOpen)}
            className={`flex h-8 w-8 items-center justify-center rounded-lg transition-colors cursor-pointer ${
              queueOpen
                ? "bg-[#33F3FF]/15 text-[#33F3FF]"
                : "text-white/60 hover:bg-white/10 hover:text-white"
            }`}
          >
            <ListMusic className="h-4 w-4" />
          </button>

          <button
            type="button"
            aria-label="Mode compact"
            title="Mode compact (C)"
            onClick={() => setCompactMode(true)}
            className="flex h-8 w-8 items-center justify-center rounded-lg text-white/60 hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
          >
            <Minimize2 className="h-4 w-4" />
          </button>
        </div>
      </div>
    </footer>
  );
}
