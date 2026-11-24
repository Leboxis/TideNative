import { create } from "zustand";
import { type TrackSummary, commands } from "@/bindings";

export type AppRoute =
  | { name: "discover" }
  | { name: "explore"; initialQuery?: string }
  | { name: "favorites" }
  | { name: "collection"; initialTab?: "albums" | "artists" | "playlists" | "favorites" }
  | { name: "album"; albumId: number }
  | { name: "artist"; artistId: number }
  | { name: "playlist"; playlistUuid: string }
  | { name: "mix"; mixId: string; title: string; subtitle?: string }
  | { name: "activity" }
  | { name: "settings" };

interface UiStore {
  route: AppRoute;
  history: AppRoute[];
  historyIndex: number;
  nowPlayingOpen: boolean;
  queueOpen: boolean;
  commandOpen: boolean;
  compactMode: boolean;
  creditsTrack: TrackSummary | null;
  addToPlaylistTrack: TrackSummary | null;
  createPlaylistOpen: boolean;
  navigate: (route: AppRoute) => void;
  goBack: () => void;
  goForward: () => void;
  canGoBack: () => boolean;
  canGoForward: () => boolean;
  setNowPlayingOpen: (open: boolean) => void;
  setQueueOpen: (open: boolean) => void;
  setCommandOpen: (open: boolean) => void;
  setCompactMode: (compact: boolean) => void;
  toggleCompactMode: () => void;
  openCredits: (track: TrackSummary | null) => void;
  openAddToPlaylist: (track: TrackSummary | null) => void;
  setCreatePlaylistOpen: (open: boolean) => void;
}

const INITIAL_ROUTE: AppRoute = { name: "discover" };

export const useUiStore = create<UiStore>((set, get) => ({
  route: INITIAL_ROUTE,
  history: [INITIAL_ROUTE],
  historyIndex: 0,
  nowPlayingOpen: false,
  queueOpen: false,
  commandOpen: false,
  compactMode: false,
  creditsTrack: null,
  addToPlaylistTrack: null,
  createPlaylistOpen: false,

  navigate: (route) =>
    set((state) => {
      const sliced = state.history.slice(0, state.historyIndex + 1);
      const nextHistory = [...sliced, route].slice(-40);
      return {
        route,
        history: nextHistory,
        historyIndex: nextHistory.length - 1,
        nowPlayingOpen: false,
      };
    }),

  goBack: () =>
    set((state) => {
      if (state.historyIndex <= 0) return state;
      const nextIndex = state.historyIndex - 1;
      return {
        historyIndex: nextIndex,
        route: state.history[nextIndex],
        nowPlayingOpen: false,
      };
    }),

  goForward: () =>
    set((state) => {
      if (state.historyIndex >= state.history.length - 1) return state;
      const nextIndex = state.historyIndex + 1;
      return {
        historyIndex: nextIndex,
        route: state.history[nextIndex],
        nowPlayingOpen: false,
      };
    }),

  canGoBack: () => get().historyIndex > 0,
  canGoForward: () => get().historyIndex < get().history.length - 1,
  setNowPlayingOpen: (nowPlayingOpen) => set({ nowPlayingOpen }),
  setQueueOpen: (queueOpen) => set({ queueOpen }),
  setCommandOpen: (commandOpen) => set({ commandOpen }),
  setCompactMode: (compactMode) => {
    set(
      compactMode
        ? {
            compactMode: true,
            nowPlayingOpen: false,
            queueOpen: false,
            commandOpen: false,
          }
        : { compactMode: false },
    );
    void commands.setCompactMode(compactMode).catch(() => {});
  },
  toggleCompactMode: () => {
    const next = !get().compactMode;
    get().setCompactMode(next);
  },
  openCredits: (creditsTrack) => set({ creditsTrack }),
  openAddToPlaylist: (addToPlaylistTrack) => set({ addToPlaylistTrack }),
  setCreatePlaylistOpen: (createPlaylistOpen) => set({ createPlaylistOpen }),
}));
