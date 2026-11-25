import { AnimatePresence, motion } from "motion/react";
import { Suspense, lazy } from "react";
import { AppShell } from "@/components/layout/AppShell";
import { AddToPlaylistDialog } from "@/components/modals/AddToPlaylistDialog";
import { CreatePlaylistDialog } from "@/components/modals/CreatePlaylistDialog";
import { CreditsDialog } from "@/components/modals/CreditsDialog";
import { ImportPlaylistDialog } from "@/components/modals/ImportPlaylistDialog";
import { ShortcutsDialog } from "@/components/modals/ShortcutsDialog";
import { NowPlayingView } from "@/components/player/NowPlayingView";
import { QueuePanel } from "@/components/player/QueuePanel";
import { CommandSearch } from "@/components/search/CommandSearch";
import { useEngineBridge } from "@/hooks/useEngineBridge";
import { useKeyboardShortcuts } from "@/hooks/useKeyboardShortcuts";
import { useUiStore } from "@/stores/uiStore";
import { DiscoverView } from "@/views/DiscoverView";

const ActivityView = lazy(() =>
  import("@/views/ActivityView").then((m) => ({ default: m.ActivityView })),
);
const AlbumDetailView = lazy(() =>
  import("@/views/AlbumDetailView").then((m) => ({ default: m.AlbumDetailView })),
);
const ArtistDetailView = lazy(() =>
  import("@/views/ArtistDetailView").then((m) => ({ default: m.ArtistDetailView })),
);
const AudioSettingsView = lazy(() =>
  import("@/views/AudioSettingsView").then((m) => ({ default: m.AudioSettingsView })),
);
const CollectionView = lazy(() =>
  import("@/views/CollectionView").then((m) => ({ default: m.CollectionView })),
);
const ExploreView = lazy(() =>
  import("@/views/ExploreView").then((m) => ({ default: m.ExploreView })),
);
const FavoritesView = lazy(() =>
  import("@/views/FavoritesView").then((m) => ({ default: m.FavoritesView })),
);
const MixDetailView = lazy(() =>
  import("@/views/MixDetailView").then((m) => ({ default: m.MixDetailView })),
);
const PlaylistDetailView = lazy(() =>
  import("@/views/PlaylistDetailView").then((m) => ({ default: m.PlaylistDetailView })),
);

function ActiveRouteView() {
  const route = useUiStore((s) => s.route);
  const routeKey =
    route.name === "album"
      ? `album-${route.albumId}`
      : route.name === "artist"
        ? `artist-${route.artistId}`
        : route.name === "playlist"
          ? `playlist-${route.playlistUuid}`
          : route.name === "mix"
            ? `mix-${route.mixId}`
            : route.name;

  const content = (() => {
    switch (route.name) {
      case "discover":
        return <DiscoverView />;
      case "explore":
        return <ExploreView initialQuery={route.initialQuery} />;
      case "favorites":
        return <FavoritesView />;
      case "collection":
        return (
          <CollectionView
            key={route.initialTab ?? "playlists"}
            initialTab={route.initialTab ?? "playlists"}
          />
        );
      case "album":
        return <AlbumDetailView albumId={route.albumId} />;
      case "artist":
        return <ArtistDetailView artistId={route.artistId} />;
      case "playlist":
        return <PlaylistDetailView playlistUuid={route.playlistUuid} />;
      case "mix":
        return (
          <MixDetailView
            mixId={route.mixId}
            title={route.title}
            subtitle={route.subtitle}
          />
        );
      case "activity":
        return <ActivityView />;
      case "settings":
        return <AudioSettingsView />;
    }
  })();

  return (
    <AnimatePresence mode="wait">
      <motion.div
        key={routeKey}
        initial={{ opacity: 0, y: 6 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0, y: -4 }}
        transition={{ duration: 0.14, ease: [0.22, 1, 0.36, 1] }}
        className="flex flex-1 min-h-0 flex-col"
      >
        <Suspense fallback={null}>{content}</Suspense>
      </motion.div>
    </AnimatePresence>
  );
}

export default function App() {
  useEngineBridge();
  useKeyboardShortcuts();

  return (
    <AppShell
      overlays={
        <>
          <NowPlayingView />
          <QueuePanel />
          <CommandSearch />
          <CreatePlaylistDialog />
          <ImportPlaylistDialog />
          <AddToPlaylistDialog />
          <CreditsDialog />
          <ShortcutsDialog />
        </>
      }
    >
      <ActiveRouteView />
    </AppShell>
  );
}
