import { motion, useMotionValue } from "motion/react";
import { memo, useEffect, useRef } from "react";
import { telemetryRuntime } from "@/lib/telemetry";
import { formatDurationMs } from "@/lib/utils";

interface ScrubberProps {
  durationMs: number;
  accentColor?: string;
  onSeek: (positionMs: number) => void;
  compact?: boolean;
}

export const Scrubber = memo(function Scrubber({
  durationMs,
  accentColor = "#33F3FF",
  onSeek,
  compact = false,
}: ScrubberProps) {
  const trackRef = useRef<HTMLDivElement | null>(null);
  const elapsedRef = useRef<HTMLSpanElement | null>(null);
  const totalRef = useRef<HTMLSpanElement | null>(null);
  const draggingRef = useRef(false);
  const lastSecRef = useRef(-1);
  const durationRef = useRef(durationMs);
  durationRef.current = durationMs;

  const progress = useMotionValue(0);
  const buffered = useMotionValue(0);

  const writeElapsed = (ms: number) => {
    const sec = Math.floor(Math.max(0, ms) / 1000);
    if (sec !== lastSecRef.current) {
      lastSecRef.current = sec;
      if (elapsedRef.current) {
        elapsedRef.current.textContent = formatDurationMs(sec * 1000);
      }
    }
  };

  useEffect(() => {
    if (totalRef.current) {
      totalRef.current.textContent = formatDurationMs(durationMs);
    }
    if (durationMs > 0) {
      const ratio = Math.min(1, Math.max(0, telemetryRuntime.positionMs / durationMs));
      progress.set(ratio);
      writeElapsed(telemetryRuntime.positionMs);
    } else {
      progress.set(0);
      buffered.set(0);
      writeElapsed(0);
    }
  }, [durationMs, progress, buffered]);

  useEffect(() => {
    return telemetryRuntime.subscribe((meta) => {
      if (draggingRef.current) return;
      const dur = meta.durationMs > 0 ? meta.durationMs : durationRef.current;
      if (dur <= 0) {
        progress.set(0);
        buffered.set(0);
        writeElapsed(0);
        return;
      }
      progress.set(Math.min(1, Math.max(0, meta.positionMs / dur)));
      buffered.set(Math.min(1, Math.max(0, (meta.positionMs + meta.bufferedMs) / dur)));
      writeElapsed(meta.positionMs);
    });
  }, [progress, buffered]);

  const ratioFromClientX = (clientX: number): number => {
    const el = trackRef.current;
    if (!el) return 0;
    const rect = el.getBoundingClientRect();
    if (rect.width <= 0) return 0;
    return Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
  };

  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (durationRef.current <= 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    draggingRef.current = true;
    const ratio = ratioFromClientX(e.clientX);
    progress.set(ratio);
    writeElapsed(Math.round(ratio * durationRef.current));
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!draggingRef.current || durationRef.current <= 0) return;
    const ratio = ratioFromClientX(e.clientX);
    progress.set(ratio);
    writeElapsed(Math.round(ratio * durationRef.current));
  };

  const handlePointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!draggingRef.current) return;
    draggingRef.current = false;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
    const ratio = ratioFromClientX(e.clientX);
    const targetMs = Math.round(ratio * durationRef.current);
    progress.set(ratio);
    writeElapsed(targetMs);
    onSeek(targetMs);
  };

  return (
    <div className="flex w-full items-center gap-2.5 select-none">
      <span
        ref={elapsedRef}
        className="w-10 text-right text-[11px] font-medium text-white/55 tabular-nums"
      >
        0:00
      </span>

      <div
        ref={trackRef}
        role="slider"
        aria-label="Position de lecture"
        aria-valuemin={0}
        aria-valuemax={Math.round(durationMs / 1000)}
        tabIndex={0}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerUp}
        className={`group relative flex flex-1 cursor-pointer items-center ${compact ? "h-4" : "h-5"}`}
      >
        <div className="relative h-1 w-full overflow-hidden rounded-full bg-white/12 group-hover:h-1.5 transition-[height] duration-150">
          <motion.div
            className="absolute inset-0 origin-left rounded-full bg-white/18 will-change-transform"
            style={{ scaleX: buffered }}
          />
          <motion.div
            className="absolute inset-0 origin-left rounded-full will-change-transform"
            style={{
              scaleX: progress,
              backgroundColor: accentColor,
            }}
          />
        </div>
      </div>

      <span
        ref={totalRef}
        className="w-10 text-left text-[11px] font-medium text-white/45 tabular-nums"
      >
        {formatDurationMs(durationMs)}
      </span>
    </div>
  );
});
