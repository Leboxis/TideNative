import { Heart, Play, Radio } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { type ArtistDetailData, commands } from "@/bindings";
import { CoverImage } from "@/components/media/CoverImage";
import { VirtualTrackList } from "@/components/media/VirtualTrackList";
import { Button } from "@/components/ui/button";
import { useLibraryStore } from "@/stores/libraryStore";
import { useUiStore } from "@/stores/uiStore";

interface ArtistDetailViewProps {
  artistId: number;
}

export function ArtistDetailView({ artistId }: ArtistDetailViewProps) {
  const [data, setData] = useState<ArtistDetailData | null>(null);
  const [loading, setLoading] = useState(true);
  const isFav = useLibraryStore((s) => s.favoriteArtistIds.has(artistId));
  const toggleFavArtistLocal = useLibraryStore((s) => s.toggleFavoriteArtistLocal);
  const navigate = useUiStore((s) => s.navigate);

  useEffect(() => {
    let active = true;
    setLoading(true);
    void commands.getArtist(artistId).then((res) => {
      if (!active) return;
      setLoading(false);
      if (res.status === "ok") setData(res.data);
      else toast.error(res.error);
    });
    return () => {
      active = false;
    };
  }, [artistId]);

  if (loading || !data) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-white/45">
        {loading ? "Chargement de l'artiste…" : "Artiste introuvable."}
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

  return (
    <div className="space-y-8 pb-8">
      <div className="flex flex-col gap-6 md:flex-row md:items-center">
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
          {data.bio && (
            <p className="line-clamp-3 max-w-3xl text-xs leading-relaxed text-white/55">
              {data.bio}
            </p>
          )}
          <div className="flex items-center gap-3 pt-2">
            {data.top_tracks.length > 0 && (
              <Button onClick={() => void commands.playTracks(data.top_tracks, 0)}>
                <Play className="h-4 w-4 fill-current" />
                Top titres
              </Button>
            )}
            <Button
              variant="secondary"
              onClick={async () => {
                const res = await commands.playArtistRadio(artistId);
                if (res.status === "error") toast.error(res.error);
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
      </div>

      {data.top_tracks.length > 0 && (
        <section className="space-y-3">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-white/50">
            Titres populaires
          </h2>
          <div className="h-[340px] flex flex-col">
            <VirtualTrackList tracks={data.top_tracks} />
          </div>
        </section>
      )}

      {data.albums.length > 0 && (
        <section className="space-y-3">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-white/50">
            Discographie
          </h2>
          <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
            {data.albums.map((album) => (
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
                <p className="truncate text-xs text-white/45 mt-0.5 tabular-nums">
                  {album.release_date?.slice(0, 4) ?? ""}
                </p>
              </button>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
