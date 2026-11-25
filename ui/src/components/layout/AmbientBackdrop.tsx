import { motion } from "motion/react";
import { useEffect, useState } from "react";
import type { CoverPalette } from "@/bindings";
import { usePlaybackStore } from "@/stores/playbackStore";

interface AmbientBackdropProps {
  palette?: CoverPalette;
  intensity?: "subtle" | "immersive";
}

function buildGradient(p: CoverPalette, intensity: "subtle" | "immersive"): string {
  if (intensity === "immersive") {
    return [
      `radial-gradient(ellipse 75% 65% at 22% 18%, ${p.primary}59 0%, transparent 70%)`,
      `radial-gradient(ellipse 65% 60% at 78% 72%, ${p.secondary}4d 0%, transparent 72%)`,
      `radial-gradient(circle at 50% 45%, ${p.surface_tint}40 0%, #040406 80%)`,
    ].join(", ");
  }
  return [
    `radial-gradient(ellipse 65% 42% at 28% -5%, ${p.primary}38 0%, transparent 72%)`,
    `radial-gradient(ellipse 50% 36% at 76% 6%, ${p.secondary}26 0%, transparent 70%)`,
    "linear-gradient(180deg, #0A0B0F 0%, #040406 32%, #040406 100%)",
  ].join(", ");
}

export function AmbientBackdrop({ palette: paletteProp, intensity = "subtle" }: AmbientBackdropProps) {
  const storePalette = usePlaybackStore((s) => s.currentPalette);
  const palette = paletteProp ?? storePalette;

  const [state, setState] = useState<{ layers: [string, string]; activeSlot: 0 | 1 }>(() => {
    const initial = buildGradient(palette, intensity);
    return { layers: [initial, initial], activeSlot: 0 };
  });

  useEffect(() => {
    const nextBg = buildGradient(palette, intensity);
    setState((prev) => {
      const currentBg = prev.layers[prev.activeSlot];
      if (currentBg === nextBg) return prev;
      const nextSlot: 0 | 1 = prev.activeSlot === 0 ? 1 : 0;
      const layers: [string, string] =
        nextSlot === 0 ? [nextBg, prev.layers[1]] : [prev.layers[0], nextBg];
      return { layers, activeSlot: nextSlot };
    });
  }, [palette.primary, palette.secondary, palette.surface_tint, intensity]);

  return (
    <div
      aria-hidden="true"
      className="pointer-events-none fixed inset-0 overflow-hidden bg-[var(--color-canvas)]"
      style={{ contain: "strict" }}
    >
      <motion.div
        className="absolute inset-0"
        style={{ backgroundImage: state.layers[0] }}
        animate={{ opacity: state.activeSlot === 0 ? 1 : 0 }}
        transition={{ duration: 0.65, ease: [0.22, 1, 0.36, 1] }}
      />
      <motion.div
        className="absolute inset-0"
        style={{ backgroundImage: state.layers[1] }}
        animate={{ opacity: state.activeSlot === 1 ? 1 : 0 }}
        transition={{ duration: 0.65, ease: [0.22, 1, 0.36, 1] }}
      />
    </div>
  );
}
