import { ChevronDown, ChevronUp, Heart, Play, Radio } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { type AlbumSummary, type ArtistDetailData, commands } from "@/bindings";
import { CoverImage } from "@/components/media/CoverImage";
import { DetailHeaderSurface } from "@/components/media/DetailHeaderSurface";
import { MediaCard } from "@/components/media/MediaCard";
import { VirtualTrackList } from "@/components/media/VirtualTrackList";
import { Button } from "@/components/ui/button";
import { CardGridSkeleton, DetailHeaderSkeleton, TrackListSkeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { quickPlayAlbum } from "@/lib/quickPlay";
import { useLibraryStore } from "@/stores/libraryStore";
import { useUiStore } from "@/stores/uiStore";

interface ArtistDetailViewProps {
  artistId: number;
}

type DiscographyTab = "albums" | "eps" | "compilations";

function AlbumGrid({
  albums,
  onSelectAlbum,
}: {
  albums: AlbumSummary[];
  onSelectAlbum: (albumId: number) => void;
}) {
  return (
    <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
      {albums.map((album) => (
        <MediaCard
          key={album.id}
          coverId={album.cover_id}
          title={album.title}
          subtitle={album.release_date?.slice(0, 4) ?? ""}
          onOpen={() => onSelectAlbum(album.id)}
          onQuickPlay={() => void quickPlayAlbum(album.id)}
          quickPlayLabel={`Lire l'album ${album.title}`}
        />
      ))}
    </div>
  );
}

export function ArtistDetailView({ artistId }: ArtistDetailViewProps) {
  const [data, setData] = useState<ArtistDetailData | null>(null);
  const [loading, setLoading] = useState(true);
  const [bioExpanded, setBioExpanded] = useState(false);
  const [discographyTab, setDiscographyTab] = useState<DiscographyTab>("albums");
  const isFav = useLibraryStore((s) => s.favoriteArtistIds.has(artistId));
  const toggleFavArtistLocal = useLibraryStore((s) => s.toggleFavoriteArtistLocal);
  const navigate = useUiStore((s) => s.navigate);

  useEffect(() => {
    let active = true;
    setLoading(true);
    setBioExpanded(false);
    void commands.getArtist(artistId).then((res) => {
      if (!active) return;
      setLoading(false);
      if (res.status === "ok") {
        setData(res.data);
        if (res.data.albums.length > 0) setDiscographyTab("albums");
        else if (res.data.eps_singles.length > 0) setDiscographyTab("eps");
        else if (res.data.compilations.length > 0) setDiscographyTab("compilations");
      } else {
        toast.error(res.error);
      }
    });
    return () => {
      active = false;
    };
  }, [artistId]);

  if (loading) {
    return (
      <div className="space-y-8 pb-8">
        <DetailHeaderSkeleton rounded="full" />
        <TrackListSkeleton rows={5} />
        <CardGridSkeleton count={6} />
      </div>
    );
  }

  if (!data) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 text-sm text-white/60">
        <p>Artiste introuvable.</p>
        <Button variant="secondary" size="sm" onClick={() => navigate({ name: "explore" })}>
          Explorer le catalogue
        </Button>
      </div>
    );
  }

  const handleFollow = async () => {
    const res = await commands.toggleFavoriteArtist(artistId, data.artist);
    if (res.status === "ok") {
      toggleFavArtistLocal(artistId, data.artist, res.data);
      toast.success(res.data ? "Artiste suivi" : "Artiste retiré");
    } else {
      toast.error(res.error);
    }
  };

  const canExpandBio = Boolean(data.bio && data.bio.length > 220);
  const discographySections: { id: DiscographyTab; label: string; items: AlbumSummary[] }[] = [
    ...(data.albums.length > 0
      ? [{ id: "albums" as const, label: `Albums (${data.albums.length})`, items: data.albums }]
      : []),
    ...(data.eps_singles.length > 0
      ? [
          {
            id: "eps" as const,
            label: `EPs & Singles (${data.eps_singles.length})`,
            items: data.eps_singles,
          },
        ]
      : []),
    ...(data.compilations.length > 0
      ? [
          {
            id: "compilations" as const,
            label: `Compilations (${data.compilations.length})`,
            items: data.compilations,
          },
        ]
      : []),
  ];

  const activeDiscographyTab = discographySections.some((s) => s.id === discographyTab)
    ? discographyTab
    : (discographySections[0]?.id ?? "albums");

  return (
    <div className="space-y-8 pb-8">
      <DetailHeaderSurface
        coverId={data.artist.picture_id ?? data.albums[0]?.cover_id}
        className="flex flex-col gap-6 md:flex-row md:items-center"
      >
        <CoverImage
          coverId={data.artist.picture_id}
          alt={data.artist.name}
          size={640}
          className="h-40 w-40 shadow-2xl"
          rounded="full"
        />
        <div className="min-w-0 flex-1 space-y-2">
          <p className="text-xs font-semibold uppercase tracking-widest text-[#33F3FF]">Artiste</p>
          <h1 className="text-3xl font-bold tracking-tight text-white">{data.artist.name}</h1>
          {data.bio ? (
            <div className="max-w-3xl space-y-1">
              <p
                className={`text-xs leading-relaxed text-white/55 ${
                  bioExpanded ? "" : "line-clamp-3"
                }`}
              >
                {data.bio}
              </p>
              {canExpandBio ? (
                <button
                  type="button"
                  onClick={() => setBioExpanded((prev) => !prev)}
                  className="inline-flex items-center gap-1 text-xs font-medium text-white/75 hover:text-white transition-colors cursor-pointer"
                >
                  {bioExpanded ? (
                    <>
                      Voir moins
                      <ChevronUp className="h-3.5 w-3.5" />
                    </>
                  ) : (
                    <>
                      Voir plus
                      <ChevronDown className="h-3.5 w-3.5" />
                    </>
                  )}
                </button>
              ) : null}
            </div>
          ) : null}
          <div className="flex items-center gap-3 pt-2">
            {data.top_tracks.length > 0 ? (
              <Button onClick={() => void commands.playTracks(data.top_tracks, 0)}>
                <Play className="h-4 w-4 fill-current" />
                Top titres
              </Button>
            ) : null}
            <Button
              variant="secondary"
              onClick={async () => {
                const res = await commands.playArtistRadio(artistId);
                if (res.status === "ok") {
                  toast.success(`Radio lancée pour ${data.artist.name}`);
                } else {
                  toast.error(res.error);
                }
              }}
            >
              <Radio className="h-4 w-4" />
              Radio artiste
            </Button>
            <Button variant="outline" onClick={handleFollow}>
              <Heart
                className={`h-4 w-4 ${isFav ? "text-[#FF2D55]" : ""}`}
                fill={isFav ? "currentColor" : "none"}
              />
              {isFav ? "Suivi" : "Suivre"}
            </Button>
          </div>
        </div>
      </DetailHeaderSurface>

      {data.top_tracks.length > 0 ? (
        <section className="space-y-3">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-white/60">
            Titres populaires
          </h2>
          <VirtualTrackList tracks={data.top_tracks} embedded />
        </section>
      ) : null}

      {discographySections.length > 0 ? (
        <section className="space-y-3">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-white/50">
            Discographie
          </h2>
          {discographySections.length === 1 ? (
            <AlbumGrid
              albums={discographySections[0].items}
              onSelectAlbum={(albumId) => navigate({ name: "album", albumId })}
            />
          ) : (
            <Tabs
              value={activeDiscographyTab}
              onValueChange={(v) => setDiscographyTab(v as DiscographyTab)}
              className="space-y-4"
            >
              <TabsList>
                {discographySections.map((section) => (
                  <TabsTrigger key={section.id} value={section.id}>
                    {section.label}
                  </TabsTrigger>
                ))}
              </TabsList>
              {discographySections.map((section) => (
                <TabsContent key={section.id} value={section.id}>
                  <AlbumGrid
                    albums={section.items}
                    onSelectAlbum={(albumId) => navigate({ name: "album", albumId })}
                  />
                </TabsContent>
              ))}
            </Tabs>
          )}
        </section>
      ) : null}

      {data.similar_artists.length > 0 ? (
        <section className="space-y-3">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-white/50">
            Artistes similaires
          </h2>
          <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
            {data.similar_artists.map((similar) => (
              <button
                key={similar.id}
                type="button"
                onClick={() => navigate({ name: "artist", artistId: similar.id })}
                className="card-oled flex flex-col items-center rounded-2xl p-4 text-center hover:border-white/20 transition-colors cursor-pointer"
              >
                <CoverImage
                  coverId={similar.picture_id}
                  alt={similar.name}
                  size={320}
                  className="h-24 w-24 mb-3"
                  rounded="full"
                />
                <p className="w-full truncate text-sm font-semibold text-white">{similar.name}</p>
              </button>
            ))}
          </div>
        </section>
      ) : null}
    </div>
  );
}
