import { Play, Radio, Search, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { type HybridSearchResults, type SearchResults, type TopHit, commands } from "@/bindings";
import { LoginGate } from "@/components/auth/LoginGate";
import { CoverImage } from "@/components/media/CoverImage";
import { MediaCard } from "@/components/media/MediaCard";
import { VirtualTrackList } from "@/components/media/VirtualTrackList";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { TrackListSkeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { quickPlayAlbum, quickPlayPlaylist } from "@/lib/quickPlay";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";

const CATEGORIES = [
  "Nouveautés",
  "Top Titres",
  "Hi-Res / Master",
  "Ambiances",
  "Hip-Hop",
  "Électronique",
  "Jazz",
  "Classique",
];

interface ExploreViewProps {
  initialQuery?: string;
}

export function ExploreView({ initialQuery = "" }: ExploreViewProps) {
  const authenticated = usePlaybackStore((s) => s.authenticated);
  const exploreHub = useLibraryStore((s) => s.exploreHub);
  const setExploreHub = useLibraryStore((s) => s.setExploreHub);
  const navigate = useUiStore((s) => s.navigate);

  const [query, setQuery] = useState(initialQuery);
  const [searchResults, setSearchResults] = useState<HybridSearchResults | null>(null);
  const [activeCategory, setActiveCategory] = useState<string | null>(null);
  const [categoryData, setCategoryData] = useState<SearchResults | null>(null);
  const [loading, setLoading] = useState(false);
  const seqRef = useRef(0);

  useEffect(() => {
    if (initialQuery) setQuery(initialQuery);
  }, [initialQuery]);

  useEffect(() => {
    if (!authenticated || exploreHub) return;
    void commands.getExploreHub().then((res) => {
      if (res.status === "ok") setExploreHub(res.data);
    });
  }, [authenticated, exploreHub, setExploreHub]);

  useEffect(() => {
    const trimmed = query.trim();
    if (!trimmed) {
      setSearchResults(null);
      return;
    }
    const mySeq = ++seqRef.current;
    const timer = window.setTimeout(async () => {
      setLoading(true);
      const res = await commands.searchHybrid(trimmed);
      if (seqRef.current !== mySeq) return;
      setLoading(false);
      if (res.status === "ok") setSearchResults(res.data);
    }, 220);
    return () => window.clearTimeout(timer);
  }, [query]);

  if (!authenticated) {
    return <LoginGate />;
  }

  const handleSelectCategory = async (cat: string) => {
    if (activeCategory === cat) {
      setActiveCategory(null);
      setCategoryData(null);
      return;
    }
    setActiveCategory(cat);
    setLoading(true);
    const res = await commands.getExploreCategory(cat);
    setLoading(false);
    if (res.status === "ok") setCategoryData(res.data);
  };

  return (
    <div className="space-y-6 pb-8">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-white">Explorer</h1>
          <p className="text-xs text-white/45 mt-0.5">
            Catalogue TIDAL et recherche hybride instantanée
          </p>
        </div>

        <div className="relative w-56 focus-within:w-72 transition-all duration-300 ease-out">
          <Search className="pointer-events-none absolute left-3 top-2.5 h-4 w-4 text-white/40" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Titres, albums, artistes…"
            className="pl-9 pr-9 h-9 focus-visible:border-[#33F3FF]/50 focus-visible:shadow-[0_0_18px_rgba(51,243,255,0.18)] transition-all duration-300"
          />
          {query && (
            <button
              type="button"
              aria-label="Effacer la recherche"
              onClick={() => setQuery("")}
              className="absolute right-2.5 top-2.5 text-white/40 hover:text-white cursor-pointer"
            >
              <X className="h-4 w-4" />
            </button>
          )}
        </div>
      </div>

      {/* Hybrid Search Results */}
      {searchResults && query.trim() ? (
        <div className="space-y-5">
          {searchResults.remote.top_hit && (
            <TopHitBanner
              topHit={searchResults.remote.top_hit}
              onNavigate={navigate}
            />
          )}

          <Tabs defaultValue="tracks" className="space-y-4">
          <TabsList>
            <TabsTrigger value="tracks">
              Titres ({searchResults.remote.tracks.length})
            </TabsTrigger>
            <TabsTrigger value="albums">
              Albums ({searchResults.remote.albums.length})
            </TabsTrigger>
            <TabsTrigger value="artists">
              Artistes ({searchResults.remote.artists.length})
            </TabsTrigger>
            <TabsTrigger value="playlists">
              Playlists ({searchResults.remote.playlists.length})
            </TabsTrigger>
          </TabsList>

          <TabsContent value="tracks">
            <VirtualTrackList tracks={searchResults.remote.tracks} embedded />
          </TabsContent>

          <TabsContent value="albums">
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
              {searchResults.remote.albums.map((album) => (
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
          </TabsContent>

          <TabsContent value="artists">
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6">
              {searchResults.remote.artists.map((artist) => (
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
                    className="h-24 w-24 mb-3"
                    rounded="full"
                  />
                  <p className="w-full truncate text-sm font-semibold text-white">{artist.name}</p>
                </button>
              ))}
            </div>
          </TabsContent>

          <TabsContent value="playlists">
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
              {searchResults.remote.playlists.map((pl) => (
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
          </TabsContent>
        </Tabs>
        </div>
      ) : (
        <>
          {/* Category Tiles with Real Cover Art */}
          <section className="space-y-3">
            <h2 className="text-sm font-semibold uppercase tracking-wider text-white/50">
              Genres & Découvertes
            </h2>
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
              {CATEGORIES.map((cat) => {
                const coverId = exploreHub?.category_covers[cat] ?? null;
                const isSelected = activeCategory === cat;
                return (
                  <button
                    key={cat}
                    type="button"
                    onClick={() => void handleSelectCategory(cat)}
                    className={`card-oled flex items-center gap-3 rounded-2xl p-3 text-left transition-colors cursor-pointer ${
                      isSelected
                        ? "border-[#33F3FF]/60 bg-[#33F3FF]/10"
                        : "hover:border-white/20"
                    }`}
                  >
                    <CoverImage
                      coverId={coverId}
                      alt={cat}
                      size={160}
                      className="h-12 w-12"
                      rounded="lg"
                    />
                    <span className="truncate text-sm font-semibold text-white">{cat}</span>
                  </button>
                );
              })}
            </div>
          </section>

          {loading && <TrackListSkeleton rows={5} />}

          {activeCategory && categoryData && (
            <section className="space-y-3">
              <div className="flex items-center justify-between">
                <h2 className="text-sm font-semibold uppercase tracking-wider text-[#33F3FF]">
                  {activeCategory}
                </h2>
                <button
                  type="button"
                  onClick={() => {
                    setActiveCategory(null);
                    setCategoryData(null);
                  }}
                  className="text-xs text-white/60 hover:text-white cursor-pointer"
                >
                  Fermer
                </button>
              </div>
              <VirtualTrackList tracks={categoryData.tracks} embedded />
            </section>
          )}

          {/* Spotlight Albums */}
          {exploreHub && exploreHub.spotlight_albums.length > 0 && (
            <section className="space-y-3">
              <h2 className="text-sm font-semibold uppercase tracking-wider text-white/60">
                Albums à la une
              </h2>
              <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
                {exploreHub.spotlight_albums.map((album) => (
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
            </section>
          )}

          {/* New Release Tracks */}
          {exploreHub && exploreHub.new_release_tracks.length > 0 && (
            <section className="space-y-3">
              <h2 className="text-sm font-semibold uppercase tracking-wider text-white/60">
                Nouvelles sorties
              </h2>
              <VirtualTrackList tracks={exploreHub.new_release_tracks} embedded />
            </section>
          )}
        </>
      )}
    </div>
  );
}

function TopHitBanner({
  topHit,
  onNavigate,
}: {
  topHit: TopHit;
  onNavigate: ReturnType<typeof useUiStore.getState>["navigate"];
}) {
  const kindLabel =
    topHit.kind === "Artist"
      ? "Artiste"
      : topHit.kind === "Album"
        ? "Album"
        : topHit.kind === "Playlist"
          ? "Playlist"
          : "Titre";

  const title =
    topHit.kind === "Artist" ? topHit.value.name : topHit.value.title;

  const subtitle =
    topHit.kind === "Track"
      ? `${topHit.value.artist_name} • ${topHit.value.album_title}`
      : topHit.kind === "Album"
        ? `${topHit.value.artist_name}${topHit.value.release_date ? ` • ${topHit.value.release_date.slice(0, 4)}` : ""}`
        : topHit.kind === "Playlist"
          ? `${topHit.value.number_of_tracks} titres`
          : null;

  const coverId =
    topHit.kind === "Artist"
      ? topHit.value.picture_id
      : topHit.kind === "Playlist"
        ? topHit.value.image_id
        : topHit.value.cover_id;

  const handleOpen = () => {
    if (topHit.kind === "Track") {
      void commands.playTracks([topHit.value], 0);
    } else if (topHit.kind === "Album") {
      onNavigate({ name: "album", albumId: topHit.value.id });
    } else if (topHit.kind === "Artist") {
      onNavigate({ name: "artist", artistId: topHit.value.id });
    } else {
      onNavigate({ name: "playlist", playlistUuid: topHit.value.uuid });
    }
  };

  const handleRadio = async () => {
    const res =
      topHit.kind === "Track"
        ? await commands.playTrackRadio(topHit.value.id)
        : topHit.kind === "Album"
          ? await commands.playAlbumRadio(topHit.value.id)
          : topHit.kind === "Artist"
            ? await commands.playArtistRadio(topHit.value.id)
            : null;
    if (res && res.status === "error") toast.error(res.error);
  };

  return (
    <section className="space-y-2.5">
      <h2 className="text-xs font-semibold uppercase tracking-wider text-white/45">
        Meilleur résultat
      </h2>
      <div className="card-oled flex flex-wrap items-center gap-5 rounded-2xl p-4 border-white/12">
        <button
          type="button"
          onClick={handleOpen}
          className="shrink-0 cursor-pointer"
          aria-label={`Ouvrir ${title}`}
        >
          <CoverImage
            coverId={coverId}
            alt={title}
            size={320}
            className="h-20 w-20 shadow-lg"
            rounded={topHit.kind === "Artist" ? "full" : "xl"}
          />
        </button>

        <div className="min-w-0 flex-1 space-y-1">
          <p className="text-[11px] font-semibold uppercase tracking-wider text-[#33F3FF]">
            {kindLabel}
          </p>
          <button
            type="button"
            onClick={handleOpen}
            className="block max-w-full truncate text-left text-xl font-bold tracking-tight text-white hover:underline cursor-pointer"
          >
            {title}
          </button>
          {subtitle && <p className="truncate text-xs text-white/55">{subtitle}</p>}
        </div>

        <div className="flex items-center gap-2">
          <Button size="sm" onClick={handleOpen}>
            <Play className="h-3.5 w-3.5 fill-current" />
            {topHit.kind === "Track" ? "Lire" : "Ouvrir"}
          </Button>
          {topHit.kind !== "Playlist" && (
            <Button size="sm" variant="secondary" onClick={() => void handleRadio()}>
              <Radio className="h-3.5 w-3.5" />
              Radio
            </Button>
          )}
        </div>
      </div>
    </section>
  );
}
