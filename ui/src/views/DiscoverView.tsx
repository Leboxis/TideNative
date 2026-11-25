import { Play, Radio, RefreshCw, Shuffle, Sparkles } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import { type TrackSummary, commands } from "@/bindings";
import { LoginGate } from "@/components/auth/LoginGate";
import { CoverImage } from "@/components/media/CoverImage";
import { DetailHeaderSurface } from "@/components/media/DetailHeaderSurface";
import { DolbyAudioLogo } from "@/components/media/DolbyAudioLogo";
import { MediaCard } from "@/components/media/MediaCard";
import { VirtualTrackList } from "@/components/media/VirtualTrackList";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  CardGridSkeleton,
  DetailHeaderSkeleton,
  Skeleton,
  TrackListSkeleton,
} from "@/components/ui/skeleton";
import { quickPlayAlbum, quickPlayMix, quickPlayPlaylist } from "@/lib/quickPlay";
import { formatSecs, hasDolbyAudio, qualityTierFromApi } from "@/lib/utils";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";

type DiscoverTab = "forYou" | "mixes" | "rediscover";

function diversifyByArtist(tracks: TrackSummary[], maxTracks: number, maxPerArtist = 1): TrackSummary[] {
  const seenIds = new Set<number>();
  const artistCounts = new Map<string, number>();
  const out: TrackSummary[] = [];

  for (const track of tracks) {
    if (out.length >= maxTracks) break;
    if (seenIds.has(track.id)) continue;
    const artistKey = track.artist_name.split(/[,&;]/)[0]?.trim().toLowerCase() ?? "";
    const count = artistCounts.get(artistKey) ?? 0;
    if (count >= maxPerArtist) continue;
    seenIds.add(track.id);
    artistCounts.set(artistKey, count + 1);
    out.push(track);
  }
  return out;
}

export function DiscoverView() {
  const authenticated = usePlaybackStore((s) => s.authenticated);
  const userMixes = useLibraryStore((s) => s.userMixes);
  const discoverTracks = useLibraryStore((s) => s.discoverTracks);
  const recentTracks = useLibraryStore((s) => s.recentTracks);
  const favorites = useLibraryStore((s) => s.favorites);
  const favoriteAlbums = useLibraryStore((s) => s.favoriteAlbums);
  const favoriteArtists = useLibraryStore((s) => s.favoriteArtists);
  const playlists = useLibraryStore((s) => s.playlists);
  const initialSyncPending = useLibraryStore((s) => s.initialSyncPending);
  const navigate = useUiStore((s) => s.navigate);

  const [syncing, setSyncing] = useState(false);
  const [refreshingDiscover, setRefreshingDiscover] = useState(false);
  const [activeTab, setActiveTab] = useState<DiscoverTab>("forYou");

  const mixTracksPool = useMemo(() => {
    const interleaved: TrackSummary[] = [];
    const maxLen = userMixes.reduce((acc, m) => Math.max(acc, m.tracks.length), 0);
    for (let i = 0; i < maxLen; i += 1) {
      for (const mix of userMixes) {
        const t = mix.tracks[i];
        if (t) interleaved.push(t);
      }
    }
    return diversifyByArtist(interleaved, 30, 2);
  }, [userMixes]);

  const rediscoverFavorites = useMemo(() => {
    if (favorites.length === 0) return [];
    const olderFirst = [...favorites].reverse();
    return diversifyByArtist(olderFirst, 24, 1);
  }, [favorites]);

  const sidebarListTracks = useMemo(
    () => (recentTracks.length > 0 ? recentTracks.slice(0, 6) : favorites.slice(0, 6)),
    [recentTracks, favorites],
  );
  const sidebarListIsRecent = recentTracks.length > 0;
  const sidebarAlbums = useMemo(() => favoriteAlbums.slice(0, 4), [favoriteAlbums]);
  const sidebarPlaylists = useMemo(() => playlists.slice(0, 4), [playlists]);
  const sidebarArtists = useMemo(() => favoriteArtists.slice(0, 6), [favoriteArtists]);

  if (!authenticated && favorites.length === 0 && userMixes.length === 0 && discoverTracks.length === 0) {
    return <LoginGate />;
  }

  const isColdLoading = initialSyncPending;

  const effectiveTab: DiscoverTab =
    activeTab === "mixes" && mixTracksPool.length === 0
      ? "forYou"
      : activeTab === "rediscover" && rediscoverFavorites.length === 0
        ? "forYou"
        : activeTab;

  const displayedTracks =
    effectiveTab === "mixes"
      ? mixTracksPool
      : effectiveTab === "rediscover"
        ? rediscoverFavorites
        : discoverTracks.length > 0
          ? discoverTracks
          : rediscoverFavorites;

  const heroTrack = discoverTracks[0] ?? mixTracksPool[0] ?? recentTracks[0] ?? null;
  const heroTier = heroTrack ? qualityTierFromApi(heroTrack.audio_quality) : null;

  const handleSync = async () => {
    setSyncing(true);
    const res = await commands.refreshLibrary();
    setSyncing(false);
    if (res.status === "ok") {
      toast.success("Bibliothèque TIDAL synchronisée");
    } else {
      toast.error(res.error);
    }
  };

  const handleVaryDiscover = async () => {
    setRefreshingDiscover(true);
    const res = await commands.refreshDiscover();
    setRefreshingDiscover(false);
    if (res.status === "ok") {
      setActiveTab("forYou");
      toast.success("Nouvelle sélection personnalisée générée");
    } else {
      toast.error(res.error);
    }
  };

  const handleHeroRadio = async () => {
    if (!heroTrack) return;
    const res = await commands.playTrackRadio(heroTrack.id);
    if (res.status === "ok") {
      toast.success(`Radio lancée d'après « ${heroTrack.title} »`);
    } else {
      toast.error(res.error);
    }
  };

  return (
    <div className="flex flex-1 min-h-0 flex-col gap-5 lg:overflow-hidden">
      {/* Header */}
      <div className="flex shrink-0 flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-white">Écouter</h1>
          <p className="text-xs text-white/60 mt-0.5">
            Découvertes personnalisées à partir de vos goûts, mix quotidiens et artistes suivis
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void handleVaryDiscover()}
            disabled={refreshingDiscover || syncing}
          >
            <Shuffle className={`h-3.5 w-3.5 ${refreshingDiscover ? "animate-pulse" : ""}`} />
            {refreshingDiscover ? "Renouvellement…" : "Varier la sélection"}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => void handleSync()}
            disabled={syncing || refreshingDiscover}
          >
            <RefreshCw className={`h-3.5 w-3.5 ${syncing ? "animate-spin" : ""}`} />
            Synchroniser
          </Button>
        </div>
      </div>

      {/* Cold initial load skeleton matching 70% / 30% split */}
      {isColdLoading ? (
        <div
          className="grid flex-1 min-h-0 grid-cols-1 gap-6 lg:grid-cols-[minmax(0,7fr)_minmax(0,3fr)] lg:overflow-hidden"
          data-testid="discover-loading-skeleton"
        >
          <div className="min-w-0 min-h-0 space-y-8 pb-8 lg:h-full lg:overflow-y-auto lg:pr-2">
            <DetailHeaderSkeleton />
            <CardGridSkeleton count={4} />
            <TrackListSkeleton rows={6} />
          </div>
          <div className="min-w-0 min-h-0 space-y-5 pb-8 lg:h-full lg:overflow-y-auto lg:pr-1">
            <div className="card-oled rounded-2xl p-4 space-y-3">
              <Skeleton className="h-4 w-32" />
              <TrackListSkeleton rows={5} />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <Skeleton className="aspect-square rounded-2xl" />
              <Skeleton className="aspect-square rounded-2xl" />
            </div>
          </div>
        </div>
      ) : (
        <div
          data-testid="discover-split-layout"
          className="grid flex-1 min-h-0 grid-cols-1 gap-6 lg:grid-cols-[minmax(0,7fr)_minmax(0,3fr)] lg:overflow-hidden"
        >
          {/* Left column (70%): Hero + Mixes + Personalized Selection */}
          <div
            data-testid="discover-primary-column"
            className="min-w-0 min-h-0 space-y-8 pb-8 lg:h-full lg:overflow-y-auto lg:pr-2 lg:overscroll-contain"
          >
            {/* Hero Cinema Card */}
            {heroTrack && (
              <DetailHeaderSurface
                coverId={heroTrack.cover_id}
                className="flex flex-col gap-5 sm:flex-row sm:items-center"
              >
                <CoverImage
                  coverId={heroTrack.cover_id}
                  alt={heroTrack.title}
                  size={640}
                  className="h-36 w-36 shadow-2xl"
                  rounded="2xl"
                />
                <div className="min-w-0 flex-1 space-y-2">
                  <div className="flex items-center gap-2">
                    {heroTier && (
                      <Badge
                        variant={heroTier === "MAX" ? "max" : heroTier === "HIGH" ? "high" : "low"}
                      >
                        {heroTier === "MAX"
                          ? "MAX · HI-RES"
                          : heroTier === "HIGH"
                            ? "HIGH · LOSSLESS"
                            : "LOW"}
                      </Badge>
                    )}
                    <span className="inline-flex items-center gap-1 text-[11px] text-white/65">
                      <Sparkles className="h-3 w-3 text-[#33F3FF]" />
                      Découverte du moment
                    </span>
                  </div>
                  <div className="flex items-center gap-2">
                    <h2 className="truncate text-2xl font-bold tracking-tight text-white">
                      {heroTrack.title}
                    </h2>
                    {hasDolbyAudio(heroTrack) && <DolbyAudioLogo size="md" />}
                  </div>
                  <p className="truncate text-sm text-white/65">
                    {heroTrack.artist_id ? (
                      <button
                        type="button"
                        onClick={() => navigate({ name: "artist", artistId: heroTrack.artist_id! })}
                        className="font-medium text-white/85 hover:text-white hover:underline cursor-pointer"
                      >
                        {heroTrack.artist_name}
                      </button>
                    ) : (
                      heroTrack.artist_name
                    )}
                    {heroTrack.album_title ? ` — ${heroTrack.album_title}` : ""}
                  </p>
                  <div className="flex flex-wrap items-center gap-2.5 pt-2">
                    <Button
                      disabled={displayedTracks.length === 0}
                      onClick={() => void commands.playTracks(displayedTracks, 0)}
                    >
                      <Play className="h-4 w-4 fill-current" />
                      Lancer l&apos;écoute
                    </Button>
                    <Button variant="secondary" onClick={() => void handleHeroRadio()}>
                      <Radio className="h-4 w-4" />
                      Radio similaire
                    </Button>
                    {heroTrack.album_id && (
                      <Button
                        variant="outline"
                        onClick={() => navigate({ name: "album", albumId: heroTrack.album_id! })}
                      >
                        Ouvrir l&apos;album
                      </Button>
                    )}
                  </div>
                </div>
              </DetailHeaderSurface>
            )}

            {/* Personalized Mixes */}
            {userMixes.length > 0 && (
              <section className="space-y-3">
                <h2 className="text-sm font-semibold uppercase tracking-wider text-white/60">
                  Vos Mix Personnalisés
                </h2>
                <div className="grid grid-cols-2 gap-3.5 sm:grid-cols-3 xl:grid-cols-4">
                  {userMixes.map((mix) => (
                    <MediaCard
                      key={mix.id}
                      coverId={mix.image_id ?? mix.tracks[0]?.cover_id}
                      title={mix.title}
                      subtitle={mix.subtitle}
                      onOpen={() =>
                        navigate({
                          name: "mix",
                          mixId: mix.id,
                          title: mix.title,
                          subtitle: mix.subtitle,
                        })
                      }
                      onQuickPlay={() => void quickPlayMix(mix.id, mix.tracks)}
                      quickPlayLabel={`Lire ${mix.title}`}
                    />
                  ))}
                </div>
              </section>
            )}

            {/* Personalized & Varied Track Selection */}
            <section className="flex flex-col space-y-3">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="flex flex-wrap items-center gap-2">
                  <h2 className="text-sm font-semibold uppercase tracking-wider text-white/60 mr-1">
                    Sélection personnalisée
                  </h2>
                  <div
                    role="tablist"
                    aria-label="Catégories de la sélection d'accueil"
                    className="inline-flex items-center gap-1 rounded-full border border-white/10 bg-white/[0.04] p-0.5"
                  >
                    <button
                      type="button"
                      role="tab"
                      aria-selected={effectiveTab === "forYou"}
                      onClick={() => setActiveTab("forYou")}
                      className={`rounded-full px-3 py-1 text-[11px] font-medium transition-colors cursor-pointer ${
                        effectiveTab === "forYou"
                          ? "bg-white text-[#050507] font-semibold"
                          : "text-white/65 hover:text-white"
                      }`}
                    >
                      Pour vous
                    </button>
                    {mixTracksPool.length > 0 && (
                      <button
                        type="button"
                        role="tab"
                        aria-selected={effectiveTab === "mixes"}
                        onClick={() => setActiveTab("mixes")}
                        className={`rounded-full px-3 py-1 text-[11px] font-medium transition-colors cursor-pointer ${
                          effectiveTab === "mixes"
                            ? "bg-white text-[#050507] font-semibold"
                            : "text-white/65 hover:text-white"
                        }`}
                      >
                        Mix &amp; Nouveautés
                      </button>
                    )}
                    {rediscoverFavorites.length > 0 && (
                      <button
                        type="button"
                        role="tab"
                        aria-selected={effectiveTab === "rediscover"}
                        onClick={() => setActiveTab("rediscover")}
                        className={`rounded-full px-3 py-1 text-[11px] font-medium transition-colors cursor-pointer ${
                          effectiveTab === "rediscover"
                            ? "bg-white text-[#050507] font-semibold"
                            : "text-white/65 hover:text-white"
                        }`}
                      >
                        À redécouvrir
                      </button>
                    )}
                  </div>
                </div>
                <span className="text-xs text-white/55 tabular-nums">
                  {displayedTracks.length} titres
                </span>
              </div>
              <VirtualTrackList
                tracks={displayedTracks}
                embedded
                emptyLabel="Cliquez sur « Varier la sélection » ou « Synchroniser » pour charger vos découvertes."
              />
            </section>
          </div>

          {/* Right column (30%): Quick track list + Album/Playlist/Artist cards */}
          <aside
            data-testid="discover-secondary-column"
            aria-label="Raccourcis et collection"
            className="min-w-0 min-h-0 space-y-5 pb-8 lg:h-full lg:overflow-y-auto lg:pr-1 lg:overscroll-contain"
          >
            {/* Track List Card */}
            {sidebarListTracks.length > 0 && (
              <section className="card-oled rounded-2xl p-4 space-y-3">
                <div className="flex items-center justify-between gap-2">
                  <h2 className="text-xs font-semibold uppercase tracking-wider text-white/65">
                    {sidebarListIsRecent ? "Écoutés récemment" : "Accès rapide favoris"}
                  </h2>
                  <button
                    type="button"
                    onClick={() =>
                      void commands.playTracks(
                        sidebarListIsRecent ? recentTracks : favorites,
                        0,
                      )
                    }
                    className="inline-flex items-center gap-1 text-[11px] font-medium text-[#33F3FF] hover:text-white transition-colors cursor-pointer"
                  >
                    <Play className="h-3 w-3 fill-current" />
                    Lire
                  </button>
                </div>

                <ul className="space-y-1">
                  {sidebarListTracks.map((track, idx) => (
                    <li key={`side-track-${track.id}-${idx}`}>
                      <button
                        type="button"
                        onClick={() =>
                          void commands.playTracks(
                            sidebarListIsRecent ? recentTracks : favorites,
                            idx,
                          )
                        }
                        className="group flex w-full items-center gap-2.5 rounded-xl px-2 py-1.5 text-left hover:bg-white/[0.06] transition-colors cursor-pointer"
                      >
                        <CoverImage
                          coverId={track.cover_id}
                          alt={track.title}
                          size={80}
                          className="h-9 w-9 shrink-0"
                          rounded="md"
                        />
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-1">
                            <p className="truncate text-xs font-medium text-white group-hover:text-[#33F3FF] transition-colors">
                              {track.title}
                            </p>
                            {hasDolbyAudio(track) && <DolbyAudioLogo />}
                          </div>
                          <p className="truncate text-[11px] text-white/55">
                            {track.artist_name}
                          </p>
                        </div>
                        <span className="shrink-0 text-[11px] text-white/45 tabular-nums">
                          {formatSecs(track.duration_secs)}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              </section>
            )}

            {/* Album Cards Grid (2 columns) */}
            {sidebarAlbums.length > 0 && (
              <section className="space-y-2.5">
                <div className="flex items-center justify-between px-0.5">
                  <h2 className="text-xs font-semibold uppercase tracking-wider text-white/60">
                    Albums sauvegardés
                  </h2>
                  <button
                    type="button"
                    onClick={() => navigate({ name: "collection", initialTab: "albums" })}
                    className="text-[11px] font-medium text-white/55 hover:text-white transition-colors cursor-pointer"
                  >
                    Voir tout
                  </button>
                </div>
                <div className="grid grid-cols-2 gap-2.5">
                  {sidebarAlbums.map((album) => (
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

            {/* Playlist Cards Grid (2 columns) */}
            {sidebarPlaylists.length > 0 && (
              <section className="space-y-2.5">
                <div className="flex items-center justify-between px-0.5">
                  <h2 className="text-xs font-semibold uppercase tracking-wider text-white/60">
                    Vos playlists
                  </h2>
                  <button
                    type="button"
                    onClick={() => navigate({ name: "collection", initialTab: "playlists" })}
                    className="text-[11px] font-medium text-white/55 hover:text-white transition-colors cursor-pointer"
                  >
                    Voir tout
                  </button>
                </div>
                <div className="grid grid-cols-2 gap-2.5">
                  {sidebarPlaylists.map((pl) => (
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
              </section>
            )}

            {/* Followed Artists Compact Cards */}
            {sidebarArtists.length > 0 && (
              <section className="card-oled rounded-2xl p-4 space-y-3">
                <div className="flex items-center justify-between">
                  <h2 className="text-xs font-semibold uppercase tracking-wider text-white/60">
                    Artistes suivis
                  </h2>
                  <button
                    type="button"
                    onClick={() => navigate({ name: "collection", initialTab: "artists" })}
                    className="text-[11px] font-medium text-white/55 hover:text-white transition-colors cursor-pointer"
                  >
                    Voir tout
                  </button>
                </div>
                <div className="grid grid-cols-2 gap-2">
                  {sidebarArtists.map((artist) => (
                    <button
                      key={artist.id}
                      type="button"
                      onClick={() => navigate({ name: "artist", artistId: artist.id })}
                      className="flex items-center gap-2 rounded-xl border border-white/[0.06] bg-white/[0.03] p-1.5 text-left hover:border-white/20 hover:bg-white/[0.06] transition-colors cursor-pointer"
                    >
                      <CoverImage
                        coverId={artist.picture_id}
                        alt={artist.name}
                        size={80}
                        className="h-8 w-8 shrink-0"
                        rounded="full"
                      />
                      <span className="truncate text-xs font-medium text-white/90">
                        {artist.name}
                      </span>
                    </button>
                  ))}
                </div>
              </section>
            )}
          </aside>
        </div>
      )}
    </div>
  );
}
