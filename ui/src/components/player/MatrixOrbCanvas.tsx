import { memo, useEffect, useRef } from "react";
import { telemetryRuntime } from "@/lib/telemetry";

interface MatrixOrbCanvasProps {
  active: boolean;
  accentColor?: string;
  size?: number;
  className?: string;
}

/**
 * 120 Hz dot-matrix audio-reactive orb inspired by `@rare-ui/matrix-orb`.
 * Reads mutable `telemetryRuntime.levels` directly inside rAF (0 React re-renders).
 */
export const MatrixOrbCanvas = memo(function MatrixOrbCanvas({
  active,
  accentColor = "#33F3FF",
  size = 36,
  className,
}: MatrixOrbCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    canvas.width = size * dpr;
    canvas.height = size * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const grid = 7;
    const step = size / (grid + 1);
    const half = size / 2;
    const maxR = size * 0.44;
    let rafId = 0;
    let phase = 0;
    let lastTime = performance.now();

    const drawFrame = (live: boolean) => {
      ctx.clearRect(0, 0, size, size);
      ctx.fillStyle = accentColor;
      const levels = telemetryRuntime.levels;
      const energy = live
        ? (levels[0] * 0.3 + levels[1] * 0.25 + levels[2] * 0.2 + levels[3] * 0.15 + levels[4] * 0.1)
        : 0;

      for (let gy = 0; gy < grid; gy++) {
        for (let gx = 0; gx < grid; gx++) {
          const cx = (gx + 1) * step;
          const cy = (gy + 1) * step;
          const dist = Math.hypot(cx - half, cy - half);
          if (dist > maxR) continue;

          const normDist = dist / maxR;
          const bandIdx = Math.min(4, Math.floor((gx / grid) * 5));
          const bandVal = live ? Math.min(1, levels[bandIdx] * 1.45) : 0;
          const wave = live
            ? 0.5 + 0.5 * Math.sin(phase * 2.8 - normDist * 4.2 + gx * 0.45)
            : 0.25;

          const alpha = live
            ? Math.min(1, 0.18 + (1 - normDist) * 0.42 + bandVal * 0.55 + energy * 0.2)
            : 0.16 + (1 - normDist) * 0.22;
          const dotR = (size / 36) * (1.05 + bandVal * 0.95 + wave * (live ? 0.35 : 0));

          ctx.beginPath();
          ctx.arc(cx, cy, dotR, 0, Math.PI * 2);
          ctx.globalAlpha = alpha;
          ctx.fill();
        }
      }
      ctx.globalAlpha = 1;
    };

    if (!active) {
      drawFrame(false);
      return;
    }

    const tick = (now: number) => {
      const dt = Math.min(50, Math.max(0, now - lastTime)) / 16.667;
      lastTime = now;
      if (!document.hidden) {
        phase += 0.045 * dt;
        drawFrame(true);
      }
      rafId = requestAnimationFrame(tick);
    };
    rafId = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(rafId);
  }, [active, accentColor, size]);

  return (
    <canvas
      ref={canvasRef}
      width={size}
      height={size}
      style={{ width: size, height: size }}
      className={className}
      aria-hidden="true"
    />
  );
});
