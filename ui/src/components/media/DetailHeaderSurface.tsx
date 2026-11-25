import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import {
  type HeaderTintPalette,
  deriveHeaderTint,
  extractTrackRowColors,
  getCachedTrackRowColors,
  hexToRgba,
} from "@/lib/covers";
import { cn } from "@/lib/utils";

interface DetailHeaderSurfaceProps {
  coverId?: string | null;
  className?: string;
  children: ReactNode;
}

export function useCoverHeaderTint(coverId: string | null | undefined): {
  tint: HeaderTintPalette;
  hasExtractedTint: boolean;
} {
  const [state, setState] = useState<{
    coverId: string | null | undefined;
    tint: HeaderTintPalette;
    hasExtractedTint: boolean;
  }>(() => {
    const cached = getCachedTrackRowColors(coverId);
    return {
      coverId,
      tint: deriveHeaderTint(cached),
      hasExtractedTint: cached !== null,
    };
  });

  useEffect(() => {
    const cached = getCachedTrackRowColors(coverId);
    if (cached) {
      setState({
        coverId,
        tint: deriveHeaderTint(cached),
        hasExtractedTint: true,
      });
      return;
    }

    setState({
      coverId,
      tint: deriveHeaderTint(null),
      hasExtractedTint: false,
    });

    if (!coverId) return;

    let alive = true;
    void extractTrackRowColors(coverId).then((extracted) => {
      if (!alive || !extracted) return;
      setState({
        coverId,
        tint: deriveHeaderTint(extracted),
        hasExtractedTint: true,
      });
    });

    return () => {
      alive = false;
    };
  }, [coverId]);

  return {
    tint: state.coverId === coverId ? state.tint : deriveHeaderTint(getCachedTrackRowColors(coverId)),
    hasExtractedTint:
      state.coverId === coverId ? state.hasExtractedTint : getCachedTrackRowColors(coverId) !== null,
  };
}

export function DetailHeaderSurface({ coverId, className, children }: DetailHeaderSurfaceProps) {
  const { tint, hasExtractedTint } = useCoverHeaderTint(coverId);

  const backgroundImage = [
    `radial-gradient(ellipse 72% 115% at 10% 18%, ${hexToRgba(tint.primary, hasExtractedTint ? 0.36 : 0.24)} 0%, transparent 68%)`,
    `radial-gradient(ellipse 58% 95% at 86% 4%, ${hexToRgba(tint.secondary, hasExtractedTint ? 0.24 : 0.16)} 0%, transparent 66%)`,
    "linear-gradient(180deg, rgba(17, 17, 22, 0.22) 0%, rgba(4, 4, 6, 0.72) 100%)",
  ].join(", ");

  const topHighlight = `linear-gradient(90deg, ${hexToRgba(
    tint.accent,
    hasExtractedTint ? 0.42 : 0.18,
  )} 0%, rgba(255, 255, 255, 0.08) 48%, transparent 100%)`;

  return (
    <div className="relative overflow-hidden rounded-3xl border border-white/[0.08] bg-[var(--color-elevated)]/75 p-5 sm:p-6 shadow-[inset_0_1px_0_rgba(255,255,255,0.06)]">
      <div
        aria-hidden="true"
        data-testid="detail-header-tint"
        data-tinted={hasExtractedTint ? "true" : "false"}
        className="pointer-events-none absolute inset-0 transition-opacity duration-500 ease-out"
        style={{ backgroundImage }}
      />
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-x-0 top-0 h-px opacity-80"
        style={{ backgroundImage: topHighlight }}
      />
      <div className={cn("relative z-10", className)}>{children}</div>
    </div>
  );
}
