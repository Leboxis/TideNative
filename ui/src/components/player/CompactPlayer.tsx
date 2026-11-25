import {
  Heart,
  Lock,
  Maximize2,
  Pause,
  Pin,
  PinOff,
  Play,
  Repeat,
  Repeat1,
  Shuffle,
  SkipBack,
  SkipForward,
  Volume2,
  VolumeX,
  X,
} from "lucide-react";
import { motion } from "motion/react";
import { useRef, useState } from "react";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { CoverImage } from "@/components/media/CoverImage";
import { DolbyAudioLogo } from "@/components/media/DolbyAudioLogo";
import { MatrixOrbCanvas } from "@/components/player/MatrixOrbCanvas";
import { Scrubber } from "@/components/player/Scrubber";
import { cn, hasDolbyAudio } from "@/lib/utils";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";

export const formatTime = (seconds = 0) => {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.floor(seconds % 60);
  return `${minutes}:${remainingSeconds.toString().padStart(2, "0")}`;
};

export interface AudioPlayerProps {
  src?: string;
  cover?: string;
  title?: string;
  showExitButton?: boolean;
  onExitCompact?: () => void;
  onQuitApp?: () => void;
}

export const AudioPlayer = ({
  src,
  cover,
  title,
  showExitButton = false,
  onExitCompact,
  onQuitApp,
}: AudioPlayerProps) => {
  const isStandaloneAudio = src !== undefined;
  const audioRef = useRef<HTMLAudioElement>(null);

  const currentTrack = usePlaybackStore((s) => s.currentTrack);
  const currentPalette = usePlaybackStore((s) => s.currentPalette);
  const playbackState = usePlaybackStore((s) => s.playbackState);
  const storeDurationMs = usePlaybackStore((s) => s.durationMs);
  const preferences = usePlaybackStore((s) => s.preferences);
  const isFav = useLibraryStore((s) => (currentTrack ? s.favoriteIds.has(currentTrack.id) : false));
  const toggleFavoriteLocal = useLibraryStore((s) => s.toggleFavoriteTrackLocal);
  const setCompactMode = useUiStore((s) => s.setCompactMode);
  const compactAlwaysOnTop = useUiStore((s) => s.compactAlwaysOnTop);
  const setCompactAlwaysOnTop = useUiStore((s) => s.setCompactAlwaysOnTop);

  const [localPlaying, setLocalPlaying] = useState(false);
  const [localTime, setLocalTime] = useState(0);
  const [localDuration, setLocalDuration] = useState(0);
  const [localShuffle, setLocalShuffle] = useState(false);
  const [localRepeat, setLocalRepeat] = useState(false);

  if (isStandaloneAudio && !src) return null;

  const isPlaying = isStandaloneAudio ? localPlaying : playbackState === "Playing";
  const isBuffering = !isStandaloneAudio && playbackState === "Buffering";
  const isShuffle = isStandaloneAudio ? localShuffle : preferences.shuffle_enabled;
  const repeatMode = isStandaloneAudio
    ? localRepeat
      ? "All"
      : "Off"
    : (preferences.repeat_mode ?? "Off");
  const isRepeat = repeatMode !== "Off";
  const repeatTitle =
    repeatMode === "One"
      ? "Répétition : titre en boucle (cliquer pour désactiver)"
      : repeatMode === "All"
        ? "Répétition : toute la file (cliquer pour répéter ce titre)"
        : "Répétition : désactivée (cliquer pour répéter la file)";

  const effectiveDurationMs =
    storeDurationMs > 0 ? storeDurationMs : (currentTrack?.duration_secs ?? 0) * 1000;

  const handleTimeUpdate = () => {
    const audio = audioRef.current;
    if (!audio) return;
    setLocalTime(audio.currentTime || 0);
    setLocalDuration(Number.isFinite(audio.duration) ? audio.duration : 0);
  };

  const togglePlay = () => {
    if (isStandaloneAudio) {
      const audio = audioRef.current;
      if (!audio) return;
      if (localPlaying) {
        audio.pause();
      } else {
        void audio.play();
      }
      setLocalPlaying(!localPlaying);
      return;
    }
    void commands.togglePlayPause();
  };

  const handleShuffle = () => {
    if (isStandaloneAudio) {
      setLocalShuffle((v) => !v);
      return;
    }
    void commands.setShuffle(!preferences.shuffle_enabled);
  };

  const handleRepeat = () => {
    if (isStandaloneAudio) {
      setLocalRepeat((v) => !v);
      return;
    }
    void commands.cycleRepeat();
  };

  const handlePrev = () => {
    if (!isStandaloneAudio) {
      void commands.prevTrack();
    }
  };

  const handleNext = () => {
    if (!isStandaloneAudio) {
      void commands.nextTrack();
    }
  };

  const handleToggleFavorite = async () => {
    if (isStandaloneAudio || !currentTrack) return;
    const res = await commands.toggleFavoriteTrack(currentTrack);
    if (res.status === "ok") {
      toggleFavoriteLocal(currentTrack, res.data);
      toast.success(res.data ? "Ajouté aux favoris" : "Retiré des favoris");
    } else {
      toast.error(res.error);
    }
  };

  const handleExit = () => {
    if (onExitCompact) {
      onExitCompact();
    } else {
      setCompactMode(false);
    }
  };

  const handleQuit = () => {
    if (onQuitApp) {
      onQuitApp();
    } else {
      void commands.quitApp();
    }
  };

  const displayTitle = isStandaloneAudio
    ? (title ?? "Audio")
    : (currentTrack?.title ?? "Prêt pour l'écoute");
  const displaySubtitle = isStandaloneAudio
    ? null
    : (currentTrack?.artist_name ?? "WASAPI Bit-Perfect Hi-Res");

  return (
    <motion.div
      data-tauri-drag-region
      initial={{ opacity: 0, scale: 0.98 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
      className={cn(
        "group relative flex flex-col justify-between overflow-hidden p-3 select-none text-[#f5f5f7]",
        isStandaloneAudio
          ? "glass-surface mx-auto w-full max-w-[280px] rounded-3xl"
          : "h-full w-full bg-[#040406]/85 backdrop-blur-xl",
      )}
    >
      {isStandaloneAudio && (
        <audio
          ref={audioRef}
          onTimeUpdate={handleTimeUpdate}
          src={src}
          className="hidden"
        />
      )}

      {/* Cover Art */}
      <div
        data-tauri-drag-region
        className="relative aspect-square w-full overflow-hidden rounded-2xl border border-white/10 bg-[#111116] shadow-2xl"
      >
        {isStandaloneAudio && cover ? (
          <img
            data-tauri-drag-region
            src={cover}
            alt={displayTitle}
            draggable={false}
            className="h-full w-full object-cover"
          />
        ) : !isStandaloneAudio && currentTrack?.cover_id ? (
          <CoverImage
            coverId={currentTrack.cover_id}
            alt={currentTrack.title}
            size={640}
            className="h-full w-full"
            rounded="2xl"
          />
        ) : (
          <div
            data-tauri-drag-region
            className="flex h-full w-full items-center justify-center bg-[#111116]"
          >
            <MatrixOrbCanvas
              active={isPlaying}
              accentColor={currentPalette.accent}
              size={72}
            />
          </div>
        )}

        {showExitButton && (
          <div className="absolute top-2.5 right-2.5 z-20 flex items-center gap-1.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity duration-150">
            <button
              type="button"
              aria-label={
                compactAlwaysOnTop
                  ? "Détacher du premier plan"
                  : "Épingler au premier plan"
              }
              aria-pressed={compactAlwaysOnTop}
              title={
                compactAlwaysOnTop
                  ? "Épinglé au premier plan (cliquer pour détacher)"
                  : "Épingler au premier plan"
              }
              onClick={(e) => {
                e.stopPropagation();
                setCompactAlwaysOnTop(!compactAlwaysOnTop);
              }}
              className={cn(
                "flex h-7 w-7 items-center justify-center rounded-full border backdrop-blur-md transition-colors cursor-pointer",
                compactAlwaysOnTop
                  ? "border-[#33F3FF]/45 bg-[#0a0b0f]/90 text-[#33F3FF] hover:bg-[#16161d]"
                  : "border-white/15 bg-[#0a0b0f]/85 text-white/75 hover:bg-[#16161d] hover:text-white",
              )}
            >
              {compactAlwaysOnTop ? (
                <Pin className="h-3.5 w-3.5" />
              ) : (
                <PinOff className="h-3.5 w-3.5" />
              )}
            </button>
            <button
              type="button"
              aria-label="Quitter le mode compact"
              title="Quitter le mode compact (Échap)"
              onClick={(e) => {
                e.stopPropagation();
                handleExit();
              }}
              className="flex h-7 w-7 items-center justify-center rounded-full border border-white/15 bg-[#0a0b0f]/85 text-white/85 hover:bg-[#16161d] hover:text-white backdrop-blur-md transition-colors cursor-pointer"
            >
              <Maximize2 className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              aria-label="Fermer l'application"
              title="Fermer l'application"
              onClick={(e) => {
                e.stopPropagation();
                handleQuit();
              }}
              className="flex h-7 w-7 items-center justify-center rounded-full border border-white/15 bg-[#0a0b0f]/85 text-white/75 hover:bg-[#ff453a] hover:border-[#ff453a] hover:text-white backdrop-blur-md transition-colors cursor-pointer"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        )}
      </div>

      {/* Track info + quick favorite/mute */}
      <div className="mt-2.5 flex items-center justify-between gap-2 px-0.5">
        <div data-tauri-drag-region className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <h3
              data-tauri-drag-region
              title={displayTitle}
              className="truncate text-sm font-semibold tracking-tight text-white"
            >
              {displayTitle}
            </h3>
            {!isStandaloneAudio && hasDolbyAudio(currentTrack) && <DolbyAudioLogo />}
          </div>
          {displaySubtitle && (
            <p
              data-tauri-drag-region
              title={displaySubtitle}
              className="truncate text-xs text-white/65"
            >
              {displaySubtitle}
            </p>
          )}
        </div>

        {!isStandaloneAudio && (
          <div className="flex items-center gap-1 shrink-0">
            {currentTrack && (
              <button
                type="button"
                aria-label={isFav ? "Retirer des favoris" : "Ajouter aux favoris"}
                onClick={(e) => {
                  e.stopPropagation();
                  void handleToggleFavorite();
                }}
                className={cn(
                  "flex h-7 w-7 items-center justify-center rounded-full transition-colors cursor-pointer",
                  isFav
                    ? "text-[#FF2D55] hover:bg-[#FF2D55]/15"
                    : "text-white/60 hover:bg-white/10 hover:text-white",
                )}
              >
                <Heart className="h-3.5 w-3.5" fill={isFav ? "currentColor" : "none"} />
              </button>
            )}
            <button
              type="button"
              aria-label={
                preferences.bit_perfect_lock
                  ? "Verrouillage Bit-Perfect actif"
                  : preferences.volume <= 0.005
                    ? "Rétablir le son"
                    : "Muet"
              }
              disabled={preferences.bit_perfect_lock}
              onClick={(e) => {
                e.stopPropagation();
                void commands.toggleMute();
              }}
              className="flex h-7 w-7 items-center justify-center rounded-full text-white/60 hover:bg-white/10 hover:text-white disabled:opacity-70 transition-colors cursor-pointer"
            >
              {preferences.bit_perfect_lock ? (
                <Lock className="h-3.5 w-3.5 text-[#30D158]" />
              ) : preferences.volume <= 0.005 ? (
                <VolumeX className="h-3.5 w-3.5" />
              ) : (
                <Volume2 className="h-3.5 w-3.5" />
              )}
            </button>
          </div>
        )}
      </div>

      {/* 120 Hz Zero-Re-render Scrubber */}
      <div className="my-1.5">
        {isStandaloneAudio ? (
          <div className="flex items-center gap-2">
            <span className="w-9 text-right text-[11px] text-white/65 tabular-nums">
              {formatTime(localTime)}
            </span>
            <input
              type="range"
              aria-label="Position de lecture"
              min={0}
              max={Math.max(1, Math.round(localDuration))}
              value={Math.round(localTime)}
              onChange={(e) => {
                const next = Number(e.target.value);
                if (audioRef.current) {
                  audioRef.current.currentTime = next;
                }
                setLocalTime(next);
              }}
              className="flex-1 accent-[#33F3FF] cursor-pointer"
            />
            <span className="w-9 text-left text-[11px] text-white/60 tabular-nums">
              {formatTime(localDuration)}
            </span>
          </div>
        ) : (
          <Scrubber
            durationMs={effectiveDurationMs}
            accentColor={currentPalette.accent}
            onSeek={(ms) => void commands.seekMs(ms)}
            compact
          />
        )}
      </div>

      {/* Transport Controls */}
      <div className="flex items-center justify-center">
        <div className="glass-surface flex items-center gap-1.5 rounded-full px-2.5 py-1.5">
          <button
            type="button"
            aria-label="Aléatoire"
            aria-pressed={isShuffle}
            onClick={(e) => {
              e.stopPropagation();
              handleShuffle();
            }}
            className={cn(
              "flex h-7 w-7 items-center justify-center rounded-full transition-colors cursor-pointer",
              isShuffle
                ? "bg-[#33F3FF]/15 text-[#33F3FF]"
                : "text-white/65 hover:bg-white/10 hover:text-white",
            )}
          >
            <Shuffle className="h-3.5 w-3.5" />
          </button>

          <button
            type="button"
            aria-label="Piste précédente"
            onClick={(e) => {
              e.stopPropagation();
              handlePrev();
            }}
            className="flex h-8 w-8 items-center justify-center rounded-full text-white/85 hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
          >
            <SkipBack className="h-4 w-4 fill-current" />
          </button>

          <button
            type="button"
            aria-label={isPlaying ? "Pause" : "Lecture"}
            onClick={(e) => {
              e.stopPropagation();
              togglePlay();
            }}
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
            onClick={(e) => {
              e.stopPropagation();
              handleNext();
            }}
            className="flex h-8 w-8 items-center justify-center rounded-full text-white/85 hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
          >
            <SkipForward className="h-4 w-4 fill-current" />
          </button>

          <button
            type="button"
            aria-label="Répétition"
            title={repeatTitle}
            aria-pressed={isRepeat}
            data-repeat-mode={repeatMode}
            onClick={(e) => {
              e.stopPropagation();
              handleRepeat();
            }}
            className={cn(
              "flex h-7 w-7 items-center justify-center rounded-full transition-colors cursor-pointer",
              isRepeat
                ? "bg-[#33F3FF]/15 text-[#33F3FF]"
                : "text-white/65 hover:bg-white/10 hover:text-white",
            )}
          >
            {repeatMode === "One" ? (
              <Repeat1 className="h-3.5 w-3.5" />
            ) : (
              <Repeat className="h-3.5 w-3.5" />
            )}
          </button>
        </div>
      </div>
    </motion.div>
  );
};

export function CompactPlayer() {
  return <AudioPlayer showExitButton />;
}

export default AudioPlayer;
