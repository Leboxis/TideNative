import { useEffect } from "react";
import { commands } from "@/bindings";
import { telemetryRuntime } from "@/lib/telemetry";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";

export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    target.isContentEditable
  );
}

const INTERACTIVE_SELECTOR =
  'button, a, summary, [role="button"], [role="switch"], [role="slider"], [role="checkbox"], [role="radio"], [role="tab"], [role="menuitem"], [role="option"], [role="combobox"]';

export function isInteractiveControl(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return Boolean(target.closest(INTERACTIVE_SELECTOR));
}

export function useKeyboardShortcuts() {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const ui = useUiStore.getState();

      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        ui.setCommandOpen(!ui.commandOpen);
        return;
      }

      if (e.key === "Escape") {
        if (ui.shortcutsOpen) {
          ui.setShortcutsOpen(false);
          return;
        }
        if (ui.commandOpen) {
          ui.setCommandOpen(false);
          return;
        }
        if (ui.creditsTrack) {
          ui.openCredits(null);
          return;
        }
        if (ui.addToPlaylistTrack) {
          ui.openAddToPlaylist(null);
          return;
        }
        if (ui.nowPlayingOpen) {
          ui.setNowPlayingOpen(false);
          return;
        }
        if (ui.queueOpen) {
          ui.setQueueOpen(false);
          return;
        }
        if (ui.compactMode) {
          ui.setCompactMode(false);
          return;
        }
      }

      if (isEditableTarget(e.target)) return;

      if (e.code === "Space" && !e.ctrlKey && !e.altKey && !e.metaKey) {
        if (isInteractiveControl(e.target)) return;
        e.preventDefault();
        void commands.togglePlayPause();
        return;
      }

      if (e.ctrlKey && e.key === "ArrowRight") {
        e.preventDefault();
        void commands.nextTrack();
        return;
      }

      if (e.ctrlKey && e.key === "ArrowLeft") {
        e.preventDefault();
        void commands.prevTrack();
        return;
      }

      if (e.altKey && e.key === "ArrowLeft") {
        e.preventDefault();
        ui.goBack();
        return;
      }

      if (e.altKey && e.key === "ArrowRight") {
        e.preventDefault();
        ui.goForward();
        return;
      }

      if (!e.ctrlKey && !e.altKey && !e.metaKey) {
        if (e.key === "/" && !e.shiftKey) {
          e.preventDefault();
          ui.setCommandOpen(true);
          return;
        }

        if (e.key === "?") {
          e.preventDefault();
          ui.setShortcutsOpen(!ui.shortcutsOpen);
          return;
        }

        if (
          (e.key === "ArrowRight" || e.key === "ArrowLeft") &&
          !isInteractiveControl(e.target)
        ) {
          const pb = usePlaybackStore.getState();
          const dur =
            telemetryRuntime.durationMs > 0 ? telemetryRuntime.durationMs : pb.durationMs;
          if (dur > 0) {
            e.preventDefault();
            const delta = e.key === "ArrowRight" ? 10_000 : -10_000;
            const nextMs = Math.min(dur, Math.max(0, telemetryRuntime.positionMs + delta));
            telemetryRuntime.seedPosition(nextMs, dur);
            void commands.seekMs(nextMs);
          }
          return;
        }

        if (
          (e.key === "ArrowUp" || e.key === "ArrowDown") &&
          !isInteractiveControl(e.target)
        ) {
          const prefs = usePlaybackStore.getState().preferences;
          if (!prefs.bit_perfect_lock) {
            e.preventDefault();
            const delta = e.key === "ArrowUp" ? 0.05 : -0.05;
            const nextVol = Math.round(Math.min(1, Math.max(0, prefs.volume + delta)) * 100) / 100;
            void commands.setVolume(nextVol);
          }
          return;
        }

        const lower = e.key.toLowerCase();
        if (lower === "m") {
          e.preventDefault();
          void commands.toggleMute();
        } else if (lower === "q") {
          e.preventDefault();
          ui.setQueueOpen(!ui.queueOpen);
        } else if (lower === "n") {
          e.preventDefault();
          ui.setNowPlayingOpen(!ui.nowPlayingOpen);
        } else if (lower === "c") {
          e.preventDefault();
          ui.toggleCompactMode();
        }
      }
    };

    const onMouseUp = (e: MouseEvent) => {
      if (e.button === 3) {
        e.preventDefault();
        useUiStore.getState().goBack();
      } else if (e.button === 4) {
        e.preventDefault();
        useUiStore.getState().goForward();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("mouseup", onMouseUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("mouseup", onMouseUp);
    };
  }, []);
}
