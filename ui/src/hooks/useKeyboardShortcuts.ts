import { useEffect } from "react";
import { commands } from "@/bindings";
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

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
