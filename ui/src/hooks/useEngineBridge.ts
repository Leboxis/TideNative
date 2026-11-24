import { useEffect } from "react";
import { toast } from "sonner";
import { commands, events } from "@/bindings";
import { telemetryRuntime } from "@/lib/telemetry";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";

export function useEngineBridge() {
  const hydratePlayback = usePlaybackStore((s) => s.hydrate);
  const setPlaybackState = usePlaybackStore((s) => s.setPlaybackState);
  const setTrackChanged = usePlaybackStore((s) => s.setTrackChanged);
  const setQueue = usePlaybackStore((s) => s.setQueue);
  const setActiveFormat = usePlaybackStore((s) => s.setActiveFormat);
  const setDevices = usePlaybackStore((s) => s.setDevices);
  const setPreferences = usePlaybackStore((s) => s.setPreferences);
  const setAuth = usePlaybackStore((s) => s.setAuth);

  const hydrateLibrary = useLibraryStore((s) => s.hydrate);
  const applySync = useLibraryStore((s) => s.applySync);

  // Reveal the frameless OLED window immediately after React's initial commit.
  // Never gate on requestAnimationFrame: WebView2 suspends rAF while HWND is hidden.
  useEffect(() => {
    const timer = window.setTimeout(() => {
      void commands.showMainWindow();
    }, 16);
    return () => window.clearTimeout(timer);
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    async function boot() {
      const bootRes = await commands.getBootstrapState();
      if (!cancelled && bootRes.status === "ok") {
        hydratePlayback(bootRes.data);
        hydrateLibrary(bootRes.data);
        if (bootRes.data.authenticated) {
          void commands.refreshLibrary();
        }
      }

      await telemetryRuntime.start();

      const stop = await events.engineEventNotice.listen((ev) => {
        const notice = ev.payload;
        switch (notice.type) {
          case "PlaybackStateChanged":
            setPlaybackState(notice.payload.state);
            break;
          case "TrackChanged":
            setTrackChanged(
              notice.payload.track,
              notice.payload.palette,
              notice.payload.queue_index,
              notice.payload.duration_ms,
            );
            break;
          case "QueueUpdated":
            setQueue(notice.payload.queue, notice.payload.queue_index);
            break;
          case "FormatNegotiated":
            setActiveFormat(notice.payload.format);
            break;
          case "DevicesEnumerated":
            setDevices(notice.payload.devices);
            break;
          case "LibrarySynced":
            applySync(notice.payload.payload);
            break;
          case "PreferencesUpdated":
            setPreferences(notice.payload.preferences);
            break;
          case "AuthChanged":
            setAuth(
              notice.payload.authenticated,
              notice.payload.user_id,
              notice.payload.country_code,
            );
            break;
          case "ErrorBanner":
            toast.error(notice.payload.message);
            break;
          case "StatusBanner":
            toast.info(notice.payload.message);
            break;
          case "TrayAction":
            break;
        }
      });

      if (cancelled) {
        stop();
      } else {
        unlisten = stop;
      }
    }

    void boot();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [
    hydratePlayback,
    hydrateLibrary,
    setPlaybackState,
    setTrackChanged,
    setQueue,
    setActiveFormat,
    setDevices,
    setPreferences,
    setAuth,
    applySync,
  ]);
}
