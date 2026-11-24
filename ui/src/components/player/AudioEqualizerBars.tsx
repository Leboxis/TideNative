import { memo, useEffect, useRef } from "react";
import { hexToRgba } from "@/lib/covers";
import { telemetryRuntime } from "@/lib/telemetry";
import { cn } from "@/lib/utils";

interface AudioEqualizerBarsProps {
  active: boolean;
  accentColor?: string;
  colors?: [string, string, string, string];
  className?: string;
}

/**
 * 120 Hz audio-reactive equalizer bars driven directly by `telemetryRuntime.levels`
 * inside `requestAnimationFrame` (zero React setState per frame), colored
 * dynamically from the active track's cover thumbnail multi-color palette.
 */
export const AudioEqualizerBars = memo(function AudioEqualizerBars({
  active,
  accentColor = "#33F3FF",
  colors,
  className,
}: AudioEqualizerBarsProps) {
  const bar0Ref = useRef<HTMLSpanElement | null>(null);
  const bar1Ref = useRef<HTMLSpanElement | null>(null);
  const bar2Ref = useRef<HTMLSpanElement | null>(null);
  const bar3Ref = useRef<HTMLSpanElement | null>(null);

  useEffect(() => {
    const bars = [bar0Ref.current, bar1Ref.current, bar2Ref.current, bar3Ref.current];
    if (!active) {
      const restScales = [0.35, 0.55, 0.42, 0.3];
      bars.forEach((bar, i) => {
        if (bar) {
          bar.style.transform = `scaleY(${restScales[i]})`;
        }
      });
      return;
    }

    let rafId = 0;
    let phase = 0;
    let lastTime = performance.now();
    const smoothed = [0.4, 0.65, 0.5, 0.35];
    const phaseOffsets = [0.0, 1.35, 2.7, 4.05];
    const phaseSpeeds = [3.4, 4.1, 3.7, 4.6];

    const tick = (now: number) => {
      const dt = Math.min(50, Math.max(0, now - lastTime)) / 16.667;
      lastTime = now;
      if (!document.hidden) {
        phase += 0.048 * dt;
        const alpha = 1 - Math.pow(1 - 0.28, dt);
        const levels = telemetryRuntime.levels;
        const energy =
          levels[0] * 0.32 +
          levels[1] * 0.28 +
          levels[2] * 0.22 +
          levels[3] * 0.12 +
          levels[4] * 0.06;

        for (let i = 0; i < 4; i++) {
          const band = Math.min(1, levels[i] * 1.65);
          const wave = 0.5 + 0.5 * Math.sin(phase * phaseSpeeds[i] + phaseOffsets[i]);
          const target =
            energy > 0.015
              ? Math.min(1, Math.max(0.2, 0.22 + band * 0.62 + wave * (0.22 + energy * 0.25)))
              : 0.24 + wave * 0.58;
          smoothed[i] += (target - smoothed[i]) * alpha;
          const el = bars[i];
          if (el) {
            el.style.transform = `scaleY(${smoothed[i].toFixed(3)})`;
          }
        }
      }
      rafId = requestAnimationFrame(tick);
    };

    rafId = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(rafId);
  }, [active]);

  const barColors: [string, string, string, string] = colors ?? [
    accentColor,
    accentColor,
    accentColor,
    accentColor,
  ];

  const makeBarStyle = (idx: 0 | 1 | 2 | 3) => {
    const c = barColors[idx] || accentColor;
    return {
      backgroundColor: c,
      boxShadow: `0 0 6px ${hexToRgba(c, 0.6)}`,
    };
  };

  return (
    <span
      className={cn("inline-flex h-3.5 items-end justify-center gap-[2px]", className)}
      aria-hidden="true"
    >
      <span
        ref={bar0Ref}
        style={makeBarStyle(0)}
        className="h-full w-[2.5px] origin-bottom rounded-full transition-colors duration-300 will-change-transform"
      />
      <span
        ref={bar1Ref}
        style={makeBarStyle(1)}
        className="h-full w-[2.5px] origin-bottom rounded-full transition-colors duration-300 will-change-transform"
      />
      <span
        ref={bar2Ref}
        style={makeBarStyle(2)}
        className="h-full w-[2.5px] origin-bottom rounded-full transition-colors duration-300 will-change-transform"
      />
      <span
        ref={bar3Ref}
        style={makeBarStyle(3)}
        className="h-full w-[2.5px] origin-bottom rounded-full transition-colors duration-300 will-change-transform"
      />
    </span>
  );
});
