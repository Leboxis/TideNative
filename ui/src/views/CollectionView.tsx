import { FolderOpen, Plus, Search, Upload, X } from "lucide-react";
import { useDeferredValue, useMemo, useState } from "react";
import { LoginGate } from "@/components/auth/LoginGate";
import { CoverImage } from "@/components/media/CoverImage";
import { MediaCard } from "@/components/media/MediaCard";
import { Button } from "@/components/ui/button";
import { quickPlayAlbum, quickPlayPlaylist } from "@/lib/quickPlay";
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

type CollectionTab = "playlists" | "albums" | "artists";
type PlaylistSort = "Default" | "TitleAsc" | "TracksDesc";
type AlbumSort = "Default" | "TitleAsc" | "ArtistAsc" | "YearDesc";
type ArtistSort = "Default" | "NameAsc" | "NameDesc";

const PLAYLIST_SORT_LABELS: Record<PlaylistSort, string> = {
  Default: "Ordre par défaut",
  TitleAsc: "Nom (A–Z)",
  TracksDesc: "Nombre de titres",
};

const ALBUM_SORT_LABELS: Record<AlbumSort, string> = {
  Default: "Ordre par défaut",
  TitleAsc: "Titre (A–Z)",
  ArtistAsc: "Artiste (A–Z)",
  YearDesc: "Année (récent)",
};

const ARTIST_SORT_LABELS: Record<ArtistSort, string> = {
  Default: "Ordre par défaut",
  NameAsc: "Nom (A–Z)",
  NameDesc: "Nom (Z–A)",
};

interface CollectionViewProps {
  initialTab?: CollectionTab;
}

export function CollectionView({ initialTab = "playlists" }: CollectionViewProps) {
  const authenticated = usePlaybackStore((s) => s.authenticated);
  const favoriteAlbums = useLibraryStore((s) => s.favoriteAlbums);
  const favoriteArtists = useLibraryStore((s) => s.favoriteArtists);
  const playlists = useLibraryStore((s) => s.playlists);
  const playlistFolders = useLibraryStore((s) => s.playlistFolders);
  const navigate = useUiStore((s) => s.navigate);
  const setCreatePlaylistOpen = useUiStore((s) => s.setCreatePlaylistOpen);
  const setImportPlaylistOpen = useUiStore((s) => s.setImportPlaylistOpen);

  const [tab, setTab] = useState<CollectionTab>(initialTab);
  const [activeFolderId, setActiveFolderId] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const deferredFilter = useDeferredValue(filter);
  const [playlistSort, setPlaylistSort] = useState<PlaylistSort>("Default");
  const [albumSort, setAlbumSort] = useState<AlbumSort>("Default");
  const [artistSort, setArtistSort] = useState<ArtistSort>("Default");

  const activeFolder = playlistFolders.find((f) => f.id === activeFolderId) ?? null;

  const visiblePlaylists = useMemo(() => {
    const base = activeFolder ? (activeFolder.playlists ?? []) : playlists;
    const q = deferredFilter.trim().toLowerCase();
    const filtered = q
      ? base.filter(
          (pl) =>
            pl.title.toLowerCase().includes(q) ||
            pl.creator.toLowerCase().includes(q) ||
            (pl.description?.toLowerCase().includes(q) ?? false),
        )
      : base;

    switch (playlistSort) {
      case "TitleAsc":
        return [...filtered].sort((a, b) => a.title.localeCompare(b.title));
      case "TracksDesc":
        return [...filtered].sort((a, b) => b.number_of_tracks - a.number_of_tracks);
      default:
        return filtered;
    }
  }, [activeFolder, playlists, deferredFilter, playlistSort]);

  const visibleAlbums = useMemo(() => {
    const q = deferredFilter.trim().toLowerCase();
    const filtered = q
      ? favoriteAlbums.filter(
          (alb) =>
            alb.title.toLowerCase().includes(q) ||
            alb.artist_name.toLowerCase().includes(q),
        )
      : favoriteAlbums;

    switch (albumSort) {
      case "TitleAsc":
        return [...filtered].sort((a, b) => a.title.localeCompare(b.title));
      case "ArtistAsc":
        return [...filtered].sort((a, b) => a.artist_name.localeCompare(b.artist_name));
      case "YearDesc":
        return [...filtered].sort((a, b) =>
          (b.release_date ?? "").localeCompare(a.release_date ?? ""),
        );
      default:
        return filtered;
    }
  }, [favoriteAlbums, deferredFilter, albumSort]);

  const visibleArtists = useMemo(() => {
    const q = deferredFilter.trim().toLowerCase();
    const filtered = q
      ? favoriteArtists.filter((art) => art.name.toLowerCase().includes(q))
      : favoriteArtists;

    switch (artistSort) {
      case "NameAsc":
        return [...filtered].sort((a, b) => a.name.localeCompare(b.name));
      case "NameDesc":
        return [...filtered].sort((a, b) => b.name.localeCompare(a.name));
      default:
        return filtered;
    }
  }, [favoriteArtists, deferredFilter, artistSort]);

  if (
    !authenticated &&
    favoriteAlbums.length === 0 &&
    favoriteArtists.length === 0 &&
    playlists.length === 0
  ) {
    return <LoginGate />;
  }

  const filterPlaceholder =
    tab === "playlists"
      ? "Filtrer vos playlists…"
      : tab === "albums"
        ? "Filtrer vos albums…"
        : "Filtrer vos artistes…";

  return (
    <div className="flex flex-1 min-h-0 flex-col space-y-5 pb-4">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-white">Collection</h1>
          <p className="text-xs text-white/45 mt-0.5 tabular-nums">
            {playlists.length} playlists • {favoriteAlbums.length} albums •{" "}
            {favoriteArtists.length} artistes
          </p>
        </div>

        <div className="flex items-center gap-2">
          {tab === "playlists" ? (
            <>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setImportPlaylistOpen(true)}
              >
                <Upload className="h-3.5 w-3.5" />
                Importer
              </Button>
              <Button size="sm" onClick={() => setCreatePlaylistOpen(true)}>
                <Plus className="h-3.5 w-3.5" />
                Nouvelle playlist
              </Button>
            </>
          ) : null}
        </div>
      </div>

      <Tabs
        value={tab}
        onValueChange={(v) => setTab(v as CollectionTab)}
        className="flex flex-1 min-h-0 flex-col"
      >
        <div className="flex flex-wrap items-center justify-between gap-3">
          <TabsList>
            <TabsTrigger value="playlists">Playlists ({playlists.length})</TabsTrigger>
            <TabsTrigger value="albums">Albums ({favoriteAlbums.length})</TabsTrigger>
            <TabsTrigger value="artists">Artistes ({favoriteArtists.length})</TabsTrigger>
          </TabsList>

          <div className="flex flex-wrap items-center gap-2">
            <div className="relative w-44 focus-within:w-60 transition-all duration-300 ease-out">
              <Search className="pointer-events-none absolute left-3 top-2.5 h-3.5 w-3.5 text-white/35" />
              <Input
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder={filterPlaceholder}
                className="pl-8 pr-8 h-8 text-xs focus-visible:border-[#33F3FF]/50 focus-visible:shadow-[0_0_16px_rgba(51,243,255,0.16)] transition-all duration-300"
              />
              {filter ? (
                <button
                  type="button"
                  aria-label="Effacer le filtre"
                  onClick={() => setFilter("")}
                  className="absolute right-2.5 top-2 text-white/40 hover:text-white cursor-pointer"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              ) : null}
            </div>

            {tab === "playlists" ? (
              <Select
                value={playlistSort}
                onValueChange={(v) => setPlaylistSort(v as PlaylistSort)}
              >
                <SelectTrigger aria-label="Trier les playlists" className="w-40 h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {(Object.keys(PLAYLIST_SORT_LABELS) as PlaylistSort[]).map((k) => (
                    <SelectItem key={k} value={k}>
                      {PLAYLIST_SORT_LABELS[k]}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : tab === "albums" ? (
              <Select value={albumSort} onValueChange={(v) => setAlbumSort(v as AlbumSort)}>
                <SelectTrigger aria-label="Trier les albums" className="w-40 h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {(Object.keys(ALBUM_SORT_LABELS) as AlbumSort[]).map((k) => (
                    <SelectItem key={k} value={k}>
                      {ALBUM_SORT_LABELS[k]}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <Select value={artistSort} onValueChange={(v) => setArtistSort(v as ArtistSort)}>
                <SelectTrigger aria-label="Trier les artistes" className="w-40 h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {(Object.keys(ARTIST_SORT_LABELS) as ArtistSort[]).map((k) => (
                    <SelectItem key={k} value={k}>
                      {ARTIST_SORT_LABELS[k]}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          </div>
        </div>

        <TabsContent value="albums" className="flex-1 overflow-y-auto">
          {visibleAlbums.length === 0 ? (
            <div className="flex flex-col items-center gap-3 py-12 text-center text-sm text-white/60">
              <p>
                {filter.trim()
                  ? "Aucun album ne correspond à votre filtre."
                  : "Aucun album sauvegardé dans votre bibliothèque."}
              </p>
              {!filter.trim() && (
                <Button variant="secondary" size="sm" onClick={() => navigate({ name: "explore" })}>
                  Explorer le catalogue
                </Button>
              )}
            </div>
          ) : (
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 pb-4">
              {visibleAlbums.map((album) => (
                <MediaCard
                  key={album.id}
                  coverId={album.cover_id}
                  title={album.title}
                  subtitle={album.artist_name}
                  onOpen={() => navigate({ name: "album", albumId: album.id })}
                  onQuickPlay={() => void quickPlayAlbum(album.id)}
                  quickPlayLabel={`Lire l'album ${album.title}`}
                />
              ))}
            </div>
          )}
        </TabsContent>

        <TabsContent value="artists" className="flex-1 overflow-y-auto">
          {visibleArtists.length === 0 ? (
            <p className="py-12 text-center text-sm text-white/45">
              {filter.trim()
                ? "Aucun artiste ne correspond à votre filtre."
                : "Aucun artiste suivi dans votre bibliothèque."}
            </p>
          ) : (
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6 pb-4">
              {visibleArtists.map((artist) => (
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
            <div className="flex flex-col items-center gap-3 py-12 text-center text-sm text-white/60">
              <p>
                {filter.trim()
                  ? "Aucune playlist ne correspond à votre filtre."
                  : "Aucune playlist dans cette vue."}
              </p>
              {!filter.trim() && (
                <Button size="sm" onClick={() => setCreatePlaylistOpen(true)}>
                  <Plus className="h-3.5 w-3.5" />
                  Créer une playlist
                </Button>
              )}
            </div>
          ) : (
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 pb-4">
              {visiblePlaylists.map((pl) => (
                <MediaCard
                  key={pl.uuid}
                  coverId={pl.image_id}
                  title={pl.title}
                  subtitle={`${pl.number_of_tracks} titres`}
                  onOpen={() => navigate({ name: "playlist", playlistUuid: pl.uuid })}
                  onQuickPlay={() => void quickPlayPlaylist(pl.uuid)}
                  quickPlayLabel={`Lire la playlist ${pl.title}`}
                />
              ))}
            </div>
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
}
