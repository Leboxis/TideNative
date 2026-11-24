"use client";

import {
  Maximize2,
  Pause,
  Play,
  Repeat,
  Shuffle,
  SkipBack,
  SkipForward,
} from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import React, { useEffect, useRef, useState } from "react";
import { commands } from "@/bindings";
import { MatrixOrbCanvas } from "@/components/player/MatrixOrbCanvas";
import { Button } from "@/components/ui/button";
import { coverUrl } from "@/lib/covers";
import { telemetryRuntime } from "@/lib/telemetry";
import { cn } from "@/lib/utils";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";

export const formatTime = (seconds: number = 0) => {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.floor(seconds % 60);
  return `${minutes}:${remainingSeconds.toString().padStart(2, "0")}`;
};

export const CustomSlider = ({
  value,
  onChange,
  className,
}: {
  value: number;
  onChange: (value: number) => void;
  className?: string;
}) => {
  return (
    <motion.div
      role="slider"
      aria-label="Position de lecture"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(value)}
      tabIndex={0}
      className={cn(
        "relative w-full h-1 bg-white/20 rounded-full cursor-pointer",
        className,
      )}
      onClick={(e) => {
        const rect = e.currentTarget.getBoundingClientRect();
        const x = e.clientX - rect.left;
        const percentage = rect.width > 0 ? (x / rect.width) * 100 : 0;
        onChange(Math.min(Math.max(percentage, 0), 100));
      }}
    >
      <motion.div
        className="absolute top-0 left-0 h-full bg-white rounded-full"
        style={{ width: `${value}%` }}
        initial={{ width: 0 }}
        animate={{ width: `${value}%` }}
        transition={{ type: "spring", stiffness: 300, damping: 30 }}
      />
    </motion.div>
  );
};

export interface AudioPlayerProps {
  src?: string;
  cover?: string;
  title?: string;
  showExitButton?: boolean;
  onExitCompact?: () => void;
}

export const AudioPlayer = ({
  src,
  cover,
  title,
  showExitButton = false,
  onExitCompact,
}: AudioPlayerProps) => {
  const isStandaloneAudio = src !== undefined;
  const audioRef = useRef<HTMLAudioElement>(null);

  const currentTrack = usePlaybackStore((s) => s.currentTrack);
  const currentPalette = usePlaybackStore((s) => s.currentPalette);
  const playbackState = usePlaybackStore((s) => s.playbackState);
  const storeDurationMs = usePlaybackStore((s) => s.durationMs);
  const preferences = usePlaybackStore((s) => s.preferences);
  const setCompactMode = useUiStore((s) => s.setCompactMode);

  const [localPlaying, setLocalPlaying] = useState(false);
  const [progress, setProgress] = useState(0);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [localShuffle, setLocalShuffle] = useState(false);
  const [localRepeat, setLocalRepeat] = useState(false);
  const lastQuarterSecRef = useRef(-1);

  const engineDurationSec =
    storeDurationMs > 0
      ? storeDurationMs / 1000
      : (currentTrack?.duration_secs ?? 0);

  useEffect(() => {
    if (isStandaloneAudio) return;

    const syncFromTelemetry = (posMs: number, durMs: number) => {
      const effectiveDurMs =
        durMs > 0
          ? durMs
          : storeDurationMs > 0
            ? storeDurationMs
            : (currentTrack?.duration_secs ?? 0) * 1000;
      const posSec = Math.max(0, posMs / 1000);
      const durSec = Math.max(0, effectiveDurMs / 1000);
      const pct =
        effectiveDurMs > 0
          ? Math.min(100, Math.max(0, (posMs / effectiveDurMs) * 100))
          : 0;
      setCurrentTime(posSec);
      setDuration(durSec);
      setProgress(Number.isFinite(pct) ? pct : 0);
    };

    lastQuarterSecRef.current = -1;
    syncFromTelemetry(telemetryRuntime.positionMs, telemetryRuntime.durationMs);

    return telemetryRuntime.subscribe((meta) => {
      const quarterSec = Math.floor(meta.positionMs / 250);
      if (quarterSec !== lastQuarterSecRef.current) {
        lastQuarterSecRef.current = quarterSec;
        syncFromTelemetry(meta.positionMs, meta.durationMs);
      }
    });
  }, [isStandaloneAudio, storeDurationMs, currentTrack?.id, currentTrack?.duration_secs]);

  const isPlaying = isStandaloneAudio
    ? localPlaying
    : playbackState === "Playing";
  const isShuffle = isStandaloneAudio
    ? localShuffle
    : preferences.shuffle_enabled;
  const isRepeat = isStandaloneAudio
    ? localRepeat
    : preferences.repeat_mode !== "Off";

  const resolvedCover =
    cover ??
    (!isStandaloneAudio
      ? (coverUrl(currentTrack?.cover_id, 640) ?? undefined)
      : undefined);

  const resolvedTitle =
    title ??
    (!isStandaloneAudio
      ? currentTrack
        ? currentTrack.artist_name
          ? `${currentTrack.artist_name} — ${currentTrack.title}`
          : currentTrack.title
        : "Prêt pour l'écoute"
      : undefined);

  const displayedDuration = isStandaloneAudio ? duration : duration || engineDurationSec;

  const togglePlay = () => {
    if (isStandaloneAudio) {
      if (audioRef.current) {
        if (localPlaying) {
          audioRef.current.pause();
        } else {
          void audioRef.current.play();
        }
        setLocalPlaying(!localPlaying);
      }
      return;
    }
    void commands.togglePlayPause();
  };

  const handleTimeUpdate = () => {
    if (audioRef.current) {
      const nextProgress =
        (audioRef.current.currentTime / audioRef.current.duration) * 100;
      setProgress(Number.isFinite(nextProgress) ? nextProgress : 0);
      setCurrentTime(audioRef.current.currentTime);
      setDuration(audioRef.current.duration);
    }
  };

  const handleSeek = (value: number) => {
    if (isStandaloneAudio) {
      if (audioRef.current && audioRef.current.duration) {
        const time = (value / 100) * audioRef.current.duration;
        if (Number.isFinite(time)) {
          audioRef.current.currentTime = time;
          setProgress(value);
        }
      }
      return;
    }
    const targetDurationSec = displayedDuration;
    if (targetDurationSec > 0) {
      const timeSec = (value / 100) * targetDurationSec;
      if (Number.isFinite(timeSec)) {
        setProgress(value);
        setCurrentTime(timeSec);
        void commands.seekMs(Math.round(timeSec * 1000));
      }
    }
  };

  const handleShuffle = () => {
    if (isStandaloneAudio) {
      setLocalShuffle(!localShuffle);
      return;
    }
    void commands.setShuffle(!preferences.shuffle_enabled);
  };

  const handleRepeat = () => {
    if (isStandaloneAudio) {
      setLocalRepeat(!localRepeat);
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

  const handleExit = () => {
    if (onExitCompact) {
      onExitCompact();
    } else {
      setCompactMode(false);
    }
  };

  if (isStandaloneAudio && !src) return null;

  return (
    <AnimatePresence>
      <motion.div
        data-tauri-drag-region
        className="group relative flex flex-col mx-auto rounded-3xl overflow-hidden bg-[#11111198] shadow-[0_0_20px_rgba(0,0,0,0.2)] backdrop-blur-sm p-3 w-[280px] h-auto select-none"
        initial={{ opacity: 0, filter: "blur(10px)" }}
        animate={{ opacity: 1, filter: "blur(0px)" }}
        exit={{ opacity: 0, filter: "blur(10px)" }}
        transition={{
          duration: 0.3,
          ease: "easeInOut",
          delay: 0.1,
          type: "spring",
        }}
        layout
      >
        {isStandaloneAudio && (
          <audio
            ref={audioRef}
            onTimeUpdate={handleTimeUpdate}
            src={src}
            className="hidden"
          />
        )}

        <motion.div
          data-tauri-drag-region
          className="flex flex-col relative"
          layout
          animate={{ opacity: 1 }}
          transition={{ duration: 0.3, ease: "easeInOut" }}
        >
          {/* Cover */}
          {(resolvedCover || !isStandaloneAudio) && (
            <motion.div
              data-tauri-drag-region
              className="bg-white/20 overflow-hidden rounded-[16px] aspect-square w-full relative"
            >
              {resolvedCover ? (
                <img
                  data-tauri-drag-region
                  src={resolvedCover}
                  alt="cover"
                  draggable={false}
                  className="!object-contain w-full my-0 p-0 !mt-0 border-none !h-full"
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
                <button
                  type="button"
                  aria-label="Quitter le mode compact"
                  title="Quitter le mode compact (Échap)"
                  onClick={(e) => {
                    e.stopPropagation();
                    handleExit();
                  }}
                  className="absolute top-2 right-2 z-20 flex h-7 w-7 items-center justify-center rounded-full bg-[#111111d1] text-white/85 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:bg-[#111111] hover:text-white transition-opacity cursor-pointer"
                >
                  <Maximize2 className="h-3.5 w-3.5" />
                </button>
              )}
            </motion.div>
          )}

          <motion.div className="flex flex-col w-full gap-y-2">
            {/* Title */}
            {resolvedTitle && (
              <motion.h3
                data-tauri-drag-region
                title={resolvedTitle}
                className="text-white font-bold text-base text-center mt-1 truncate"
              >
                {resolvedTitle}
              </motion.h3>
            )}

            {/* Slider */}
            <motion.div className="flex flex-col gap-y-1">
              <CustomSlider
                value={progress}
                onChange={handleSeek}
                className="w-full"
              />
              <div className="flex items-center justify-between">
                <span className="text-white text-sm tabular-nums">
                  {formatTime(currentTime)}
                </span>
                <span className="text-white text-sm tabular-nums">
                  {formatTime(displayedDuration)}
                </span>
              </div>
            </motion.div>

            {/* Controls */}
            <motion.div className="flex items-center justify-center w-full">
              <div className="flex items-center gap-2 w-fit bg-[#11111198] rounded-[16px] p-2">
                <motion.div
                  whileHover={{ scale: 1.1 }}
                  whileTap={{ scale: 0.9 }}
                >
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label="Aléatoire"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleShuffle();
                    }}
                    className={cn(
                      "text-white hover:bg-[#111111d1] hover:text-white h-8 w-8 rounded-full",
                      isShuffle && "bg-[#111111d1] text-white",
                    )}
                  >
                    <Shuffle className="h-5 w-5" />
                  </Button>
                </motion.div>
                <motion.div
                  whileHover={{ scale: 1.1 }}
                  whileTap={{ scale: 0.9 }}
                >
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label="Piste précédente"
                    onClick={(e) => {
                      e.stopPropagation();
                      handlePrev();
                    }}
                    className="text-white hover:bg-[#111111d1] hover:text-white h-8 w-8 rounded-full"
                  >
                    <SkipBack className="h-5 w-5" />
                  </Button>
                </motion.div>
                <motion.div
                  whileHover={{ scale: 1.1 }}
                  whileTap={{ scale: 0.9 }}
                >
                  <Button
                    onClick={(e) => {
                      e.stopPropagation();
                      togglePlay();
                    }}
                    variant="ghost"
                    size="icon"
                    aria-label={isPlaying ? "Pause" : "Lecture"}
                    className="text-white hover:bg-[#111111d1] hover:text-white h-8 w-8 rounded-full"
                  >
                    {isPlaying ? (
                      <Pause className="h-5 w-5" />
                    ) : (
                      <Play className="h-5 w-5" />
                    )}
                  </Button>
                </motion.div>
                <motion.div
                  whileHover={{ scale: 1.1 }}
                  whileTap={{ scale: 0.9 }}
                >
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label="Piste suivante"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleNext();
                    }}
                    className="text-white hover:bg-[#111111d1] hover:text-white h-8 w-8 rounded-full"
                  >
                    <SkipForward className="h-5 w-5" />
                  </Button>
                </motion.div>

                <motion.div
                  whileHover={{ scale: 1.1 }}
                  whileTap={{ scale: 0.9 }}
                >
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label="Répétition"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleRepeat();
                    }}
                    className={cn(
                      "text-white hover:bg-[#111111d1] hover:text-white h-8 w-8 rounded-full",
                      isRepeat && "bg-[#111111d1] text-white",
                    )}
                  >
                    <Repeat className="h-5 w-5" />
                  </Button>
                </motion.div>
              </div>
            </motion.div>
          </motion.div>
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
};

export function CompactPlayer() {
  return <AudioPlayer showExitButton />;
}

export default AudioPlayer;
