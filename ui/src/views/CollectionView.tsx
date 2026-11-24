import { FolderOpen, Play, Plus, Search } from "lucide-react";
import { useDeferredValue, useMemo, useState } from "react";
import { commands } from "@/bindings";
import { LoginGate } from "@/components/auth/LoginGate";
import { CoverImage } from "@/components/media/CoverImage";
import { VirtualTrackList } from "@/components/media/VirtualTrackList";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";

type TrackSortOrder = "Default" | "TitleAsc" | "ArtistAsc" | "AlbumAsc" | "DurationDesc";

const SORT_LABELS: Record<TrackSortOrder, string> = {
  Default: "Ordre d'ajout",
  TitleAsc: "Titre (A–Z)",
  ArtistAsc: "Artiste (A–Z)",
  AlbumAsc: "Album (A–Z)",
  DurationDesc: "Durée",
};

interface CollectionViewProps {
  initialTab?: "favorites" | "albums" | "artists" | "playlists";
}

export function CollectionView({ initialTab = "favorites" }: CollectionViewProps) {
  const authenticated = usePlaybackStore((s) => s.authenticated);
  const favorites = useLibraryStore((s) => s.favorites);
  const favoriteAlbums = useLibraryStore((s) => s.favoriteAlbums);
  const favoriteArtists = useLibraryStore((s) => s.favoriteArtists);
  const playlists = useLibraryStore((s) => s.playlists);
  const playlistFolders = useLibraryStore((s) => s.playlistFolders);
  const navigate = useUiStore((s) => s.navigate);
  const setCreatePlaylistOpen = useUiStore((s) => s.setCreatePlaylistOpen);

  const [tab, setTab] = useState(initialTab);
  const [filter, setFilter] = useState("");
  const deferredFilter = useDeferredValue(filter);
  const [sort, setSort] = useState<TrackSortOrder>("Default");
  const [activeFolderId, setActiveFolderId] = useState<string | null>(null);

  const displayedFavorites = useMemo(() => {
    const q = deferredFilter.trim().toLowerCase();
    const list = q
      ? favorites.filter(
          (t) =>
            t.title.toLowerCase().includes(q) ||
            t.artist_name.toLowerCase().includes(q) ||
            t.album_title.toLowerCase().includes(q),
        )
      : favorites;

    switch (sort) {
      case "TitleAsc":
        return [...list].sort((a, b) => a.title.localeCompare(b.title));
      case "ArtistAsc":
        return [...list].sort((a, b) => a.artist_name.localeCompare(b.artist_name));
      case "AlbumAsc":
        return [...list].sort((a, b) => a.album_title.localeCompare(b.album_title));
      case "DurationDesc":
        return [...list].sort((a, b) => b.duration_secs - a.duration_secs);
      default:
        return list;
    }
  }, [favorites, deferredFilter, sort]);

  if (
    !authenticated &&
    favorites.length === 0 &&
    favoriteAlbums.length === 0 &&
    favoriteArtists.length === 0 &&
    playlists.length === 0
  ) {
    return <LoginGate />;
  }

  const activeFolder = playlistFolders.find((f) => f.id === activeFolderId) ?? null;
  const visiblePlaylists = activeFolder ? (activeFolder.playlists ?? []) : playlists;

  return (
    <div className="flex flex-1 min-h-0 flex-col space-y-5 pb-4">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-white">Collection</h1>
          <p className="text-xs text-white/45 mt-0.5 tabular-nums">
            {favorites.length} titres • {favoriteAlbums.length} albums • {favoriteArtists.length}{" "}
            artistes • {playlists.length} playlists
          </p>
        </div>

        <div className="flex items-center gap-2">
          {tab === "favorites" && displayedFavorites.length > 0 && (
            <Button size="sm" onClick={() => void commands.playTracks(displayedFavorites, 0)}>
              <Play className="h-3.5 w-3.5 fill-current" />
              Tout lire
            </Button>
          )}
          {tab === "playlists" && (
            <Button size="sm" onClick={() => setCreatePlaylistOpen(true)}>
              <Plus className="h-3.5 w-3.5" />
              Nouvelle playlist
            </Button>
          )}
        </div>
      </div>

      <Tabs
        value={tab}
        onValueChange={(v) => setTab(v as typeof tab)}
        className="flex flex-1 min-h-0 flex-col"
      >
        <div className="flex flex-wrap items-center justify-between gap-3">
          <TabsList>
            <TabsTrigger value="favorites">Titres ({favorites.length})</TabsTrigger>
            <TabsTrigger value="albums">Albums ({favoriteAlbums.length})</TabsTrigger>
            <TabsTrigger value="artists">Artistes ({favoriteArtists.length})</TabsTrigger>
            <TabsTrigger value="playlists">Playlists ({playlists.length})</TabsTrigger>
          </TabsList>

          {tab === "favorites" && (
            <div className="flex items-center gap-2">
              <div className="relative w-56">
                <Search className="pointer-events-none absolute left-3 top-2.5 h-3.5 w-3.5 text-white/35" />
                <Input
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                  placeholder="Filtrer vos favoris…"
                  className="pl-8 h-8 text-xs"
                />
              </div>
              <Select value={sort} onValueChange={(v) => setSort(v as TrackSortOrder)}>
                <SelectTrigger className="w-40 h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {(Object.keys(SORT_LABELS) as TrackSortOrder[]).map((k) => (
                    <SelectItem key={k} value={k}>
                      {SORT_LABELS[k]}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}
        </div>

        <TabsContent value="favorites" className="flex flex-1 min-h-0 flex-col">
          <VirtualTrackList tracks={displayedFavorites} />
        </TabsContent>

        <TabsContent value="albums" className="flex-1 overflow-y-auto">
          {favoriteAlbums.length === 0 ? (
            <p className="py-12 text-center text-sm text-white/45">
              Aucun album sauvegardé dans votre bibliothèque.
            </p>
          ) : (
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 pb-4">
              {favoriteAlbums.map((album) => (
                <button
                  key={album.id}
                  type="button"
                  onClick={() => navigate({ name: "album", albumId: album.id })}
                  className="group card-oled flex flex-col rounded-2xl p-3 text-left hover:border-white/20 transition-colors cursor-pointer"
                >
                  <CoverImage
                    coverId={album.cover_id}
                    alt={album.title}
                    size={320}
                    className="aspect-square w-full mb-3"
                    rounded="xl"
                  />
                  <p className="truncate text-sm font-semibold text-white">{album.title}</p>
                  <p className="truncate text-xs text-white/45 mt-0.5">{album.artist_name}</p>
                </button>
              ))}
            </div>
          )}
        </TabsContent>

        <TabsContent value="artists" className="flex-1 overflow-y-auto">
          {favoriteArtists.length === 0 ? (
            <p className="py-12 text-center text-sm text-white/45">
              Aucun artiste suivi dans votre bibliothèque.
            </p>
          ) : (
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6 pb-4">
              {favoriteArtists.map((artist) => (
                <button
                  key={artist.id}
                  type="button"
                  onClick={() => navigate({ name: "artist", artistId: artist.id })}
                  className="card-oled flex flex-col items-center rounded-2xl p-4 text-center hover:border-white/20 transition-colors cursor-pointer"
                >
                  <CoverImage
                    coverId={artist.picture_id}
                    alt={artist.name}
                    size={320}
                    className="h-28 w-28 mb-3"
                    rounded="full"
                  />
                  <p className="w-full truncate text-sm font-semibold text-white">{artist.name}</p>
                </button>
              ))}
            </div>
          )}
        </TabsContent>

        <TabsContent value="playlists" className="flex-1 overflow-y-auto space-y-4">
          {playlistFolders.length > 0 ? (
            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                onClick={() => setActiveFolderId(null)}
                className={`rounded-full px-3 py-1 text-xs font-medium border transition-colors cursor-pointer ${
                  activeFolderId === null
                    ? "bg-white text-[#050507] border-white"
                    : "bg-white/[0.04] text-white/70 border-white/10 hover:border-white/20"
                }`}
              >
                Toutes ({playlists.length})
              </button>
              {playlistFolders.map((folder) => (
                <button
                  key={folder.id}
                  type="button"
                  onClick={() => setActiveFolderId(folder.id)}
                  className={`inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-medium border transition-colors cursor-pointer ${
                    activeFolderId === folder.id
                      ? "bg-white text-[#050507] border-white"
                      : "bg-white/[0.04] text-white/70 border-white/10 hover:border-white/20"
                  }`}
                >
                  <FolderOpen className="h-3 w-3" />
                  {folder.name} ({(folder.playlists ?? []).length})
                </button>
              ))}
            </div>
          ) : null}

          {visiblePlaylists.length === 0 ? (
            <p className="py-12 text-center text-sm text-white/45">
              Aucune playlist dans cette vue.
            </p>
          ) : (
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 pb-4">
              {visiblePlaylists.map((pl) => (
                <button
                  key={pl.uuid}
                  type="button"
                  onClick={() => navigate({ name: "playlist", playlistUuid: pl.uuid })}
                  className="group card-oled flex flex-col rounded-2xl p-3 text-left hover:border-white/20 transition-colors cursor-pointer"
                >
                  <CoverImage
                    coverId={pl.image_id}
                    alt={pl.title}
                    size={320}
                    className="aspect-square w-full mb-3"
                    rounded="xl"
                  />
                  <p className="truncate text-sm font-semibold text-white">{pl.title}</p>
                  <p className="truncate text-xs text-white/45 mt-0.5 tabular-nums">
                    {pl.number_of_tracks} titres
                  </p>
                </button>
              ))}
            </div>
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
}
