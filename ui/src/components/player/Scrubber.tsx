import { motion, useMotionValue, useTransform } from "motion/react";
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
  const thumbRef = useRef<HTMLSpanElement | null>(null);
  const previewRef = useRef<HTMLDivElement | null>(null);
  const draggingRef = useRef(false);
  const lastSecRef = useRef(-1);
  const durationRef = useRef(durationMs);
  durationRef.current = durationMs;

  const progress = useMotionValue(0);
  const buffered = useMotionValue(0);
  const thumbLeft = useTransform(progress, (v) => `${(v * 100).toFixed(3)}%`);

  const writeElapsed = (ms: number) => {
    const sec = Math.floor(Math.max(0, ms) / 1000);
    if (sec !== lastSecRef.current) {
      lastSecRef.current = sec;
      const formatted = formatDurationMs(sec * 1000);
      if (elapsedRef.current) {
        elapsedRef.current.textContent = formatted;
      }
      if (trackRef.current) {
        trackRef.current.setAttribute("aria-valuenow", String(sec));
        trackRef.current.setAttribute("aria-valuetext", formatted);
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

  const updatePreview = (clientX: number) => {
    const preview = previewRef.current;
    if (!preview || durationRef.current <= 0) return;
    const ratio = ratioFromClientX(clientX);
    const ms = Math.round(ratio * durationRef.current);
    preview.textContent = formatDurationMs(ms);
    preview.style.left = `${(ratio * 100).toFixed(2)}%`;
    preview.style.opacity = "1";
  };

  const hidePreview = () => {
    if (previewRef.current) {
      previewRef.current.style.opacity = "0";
    }
  };

  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (durationRef.current <= 0) return;
    if (typeof e.currentTarget.setPointerCapture === "function") {
      e.currentTarget.setPointerCapture(e.pointerId);
    }
    draggingRef.current = true;
    thumbRef.current?.style.setProperty("opacity", "1");
    const ratio = ratioFromClientX(e.clientX);
    progress.set(ratio);
    writeElapsed(Math.round(ratio * durationRef.current));
    updatePreview(e.clientX);
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (durationRef.current <= 0) return;
    updatePreview(e.clientX);
    if (!draggingRef.current) return;
    const ratio = ratioFromClientX(e.clientX);
    progress.set(ratio);
    writeElapsed(Math.round(ratio * durationRef.current));
  };

  const handlePointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!draggingRef.current) return;
    draggingRef.current = false;
    thumbRef.current?.style.removeProperty("opacity");
    if (
      typeof e.currentTarget.hasPointerCapture === "function" &&
      e.currentTarget.hasPointerCapture(e.pointerId)
    ) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
    const ratio = ratioFromClientX(e.clientX);
    const targetMs = Math.round(ratio * durationRef.current);
    progress.set(ratio);
    writeElapsed(targetMs);
    onSeek(targetMs);
  };

  const handlePointerCancel = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!draggingRef.current) return;
    draggingRef.current = false;
    thumbRef.current?.style.removeProperty("opacity");
    hidePreview();
    if (
      typeof e.currentTarget.hasPointerCapture === "function" &&
      e.currentTarget.hasPointerCapture(e.pointerId)
    ) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
    const dur = durationRef.current;
    if (dur > 0) {
      const liveRatio = Math.min(1, Math.max(0, telemetryRuntime.positionMs / dur));
      progress.set(liveRatio);
      writeElapsed(telemetryRuntime.positionMs);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const dur = durationRef.current;
    if (dur <= 0) return;
    const currentMs = Math.round(progress.get() * dur);
    const stepMs = e.shiftKey ? 10_000 : 5_000;
    let targetMs: number | null = null;

    switch (e.key) {
      case "ArrowRight":
      case "ArrowUp":
        targetMs = Math.min(dur, currentMs + stepMs);
        break;
      case "ArrowLeft":
      case "ArrowDown":
        targetMs = Math.max(0, currentMs - stepMs);
        break;
      case "PageUp":
        targetMs = Math.min(dur, currentMs + 15_000);
        break;
      case "PageDown":
        targetMs = Math.max(0, currentMs - 15_000);
        break;
      case "Home":
        targetMs = 0;
        break;
      case "End":
        targetMs = dur;
        break;
      default:
        return;
    }

    e.preventDefault();
    e.stopPropagation();
    progress.set(targetMs / dur);
    writeElapsed(targetMs);
    onSeek(targetMs);
  };

  return (
    <div className="flex w-full items-center gap-2.5 select-none">
      <span
        ref={elapsedRef}
        className="w-10 text-right text-[11px] font-medium text-white/65 tabular-nums"
      >
        0:00
      </span>

      <div
        ref={trackRef}
        role="slider"
        aria-label="Position de lecture"
        aria-valuemin={0}
        aria-valuemax={Math.round(durationMs / 1000)}
        aria-valuenow={0}
        aria-valuetext="0:00"
        tabIndex={0}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerCancel}
        onPointerLeave={() => {
          if (!draggingRef.current) hidePreview();
        }}
        onKeyDown={handleKeyDown}
        className={`group relative flex flex-1 touch-none cursor-pointer items-center rounded-full focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#33F3FF] focus-visible:ring-offset-2 focus-visible:ring-offset-[#040406] ${
          compact ? "h-4" : "h-5"
        }`}
      >
        <div
          ref={previewRef}
          aria-hidden="true"
          style={{ opacity: 0, left: "0%" }}
          className="pointer-events-none absolute -top-6 z-20 -translate-x-1/2 rounded-md border border-white/15 bg-[#111116]/95 px-1.5 py-0.5 text-[10px] font-medium text-white shadow-lg transition-opacity duration-100 tabular-nums"
        />

        <div className="relative h-1 w-full overflow-hidden rounded-full bg-white/15 group-hover:h-1.5 group-focus-visible:h-1.5 transition-[height] duration-150">
          <motion.div
            className="absolute inset-0 origin-left rounded-full bg-white/20 will-change-transform"
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

        <motion.span
          ref={thumbRef}
          aria-hidden="true"
          style={{ left: thumbLeft, backgroundColor: accentColor }}
          className="pointer-events-none absolute top-1/2 h-3 w-3 -translate-x-1/2 -translate-y-1/2 rounded-full opacity-0 shadow-[0_1px_6px_rgba(0,0,0,0.65)] ring-2 ring-[#040406] transition-opacity duration-150 group-hover:opacity-100 group-focus-visible:opacity-100"
        />
      </div>

      <span
        ref={totalRef}
        className="w-10 text-left text-[11px] font-medium text-white/60 tabular-nums"
      >
        {formatDurationMs(durationMs)}
      </span>
    </div>
  );
});
