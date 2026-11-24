import { Search, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { type HybridSearchResults, type SearchResults, commands } from "@/bindings";
import { LoginGate } from "@/components/auth/LoginGate";
import { CoverImage } from "@/components/media/CoverImage";
import { VirtualTrackList } from "@/components/media/VirtualTrackList";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
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
            Catalogue complet TIDAL Hi-Res et recherche hybride instantanée
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
            <div className="h-[460px] flex flex-col">
              <VirtualTrackList tracks={searchResults.remote.tracks} />
            </div>
          </TabsContent>

          <TabsContent value="albums">
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
              {searchResults.remote.albums.map((album) => (
                <button
                  key={album.id}
                  type="button"
                  onClick={() => navigate({ name: "album", albumId: album.id })}
                  className="card-oled flex flex-col rounded-2xl p-3 text-left hover:border-white/20 transition-colors cursor-pointer"
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
                <button
                  key={pl.uuid}
                  type="button"
                  onClick={() => navigate({ name: "playlist", playlistUuid: pl.uuid })}
                  className="card-oled flex flex-col rounded-2xl p-3 text-left hover:border-white/20 transition-colors cursor-pointer"
                >
                  <CoverImage
                    coverId={pl.image_id}
                    alt={pl.title}
                    size={320}
                    className="aspect-square w-full mb-3"
                    rounded="xl"
                  />
                  <p className="truncate text-sm font-semibold text-white">{pl.title}</p>
                  <p className="truncate text-xs text-white/45 mt-0.5">
                    {pl.number_of_tracks} titres
                  </p>
                </button>
              ))}
            </div>
          </TabsContent>
        </Tabs>
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

          {loading && (
            <p className="text-xs text-white/45">Chargement du catalogue TIDAL…</p>
          )}

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
                  className="text-xs text-white/45 hover:text-white cursor-pointer"
                >
                  Fermer
                </button>
              </div>
              <div className="h-[380px] flex flex-col">
                <VirtualTrackList tracks={categoryData.tracks} />
              </div>
            </section>
          )}

          {/* Spotlight Albums */}
          {exploreHub && exploreHub.spotlight_albums.length > 0 && (
            <section className="space-y-3">
              <h2 className="text-sm font-semibold uppercase tracking-wider text-white/50">
                Albums à la une
              </h2>
              <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
                {exploreHub.spotlight_albums.map((album) => (
                  <button
                    key={album.id}
                    type="button"
                    onClick={() => navigate({ name: "album", albumId: album.id })}
                    className="card-oled flex flex-col rounded-2xl p-3 text-left hover:border-white/20 transition-colors cursor-pointer"
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
            </section>
          )}

          {/* New Release Tracks */}
          {exploreHub && exploreHub.new_release_tracks.length > 0 && (
            <section className="space-y-3">
              <h2 className="text-sm font-semibold uppercase tracking-wider text-white/50">
                Nouvelles sorties
              </h2>
              <div className="h-[380px] flex flex-col">
                <VirtualTrackList tracks={exploreHub.new_release_tracks} />
              </div>
            </section>
          )}
        </>
      )}
    </div>
  );
}
