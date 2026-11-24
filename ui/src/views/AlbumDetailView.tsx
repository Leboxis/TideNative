import { Heart, Play, Shuffle } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { type AlbumDetailData, commands } from "@/bindings";
import { CoverImage } from "@/components/media/CoverImage";
import { VirtualTrackList } from "@/components/media/VirtualTrackList";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { formatSecs, qualityTierFromApi } from "@/lib/utils";
import { useLibraryStore } from "@/stores/libraryStore";
import { useUiStore } from "@/stores/uiStore";

interface AlbumDetailViewProps {
  albumId: number;
}

export function AlbumDetailView({ albumId }: AlbumDetailViewProps) {
  const [data, setData] = useState<AlbumDetailData | null>(null);
  const [loading, setLoading] = useState(true);
  const isFav = useLibraryStore((s) => s.favoriteAlbumIds.has(albumId));
  const toggleFavAlbumLocal = useLibraryStore((s) => s.toggleFavoriteAlbumLocal);
  const navigate = useUiStore((s) => s.navigate);

  useEffect(() => {
    let active = true;
    setLoading(true);
    void commands.getAlbum(albumId).then((res) => {
      if (!active) return;
      setLoading(false);
      if (res.status === "ok") setData(res.data);
      else toast.error(res.error);
    });
    return () => {
      active = false;
    };
  }, [albumId]);

  if (loading || !data) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-white/45">
        {loading ? "Chargement de l'album…" : "Album introuvable."}
      </div>
    );
  }

  const totalSecs = data.tracks.reduce((acc, t) => acc + t.duration_secs, 0);
  const tier = qualityTierFromApi(data.album.audio_quality);

  const handleToggleFav = async () => {
    const res = await commands.toggleFavoriteAlbum(albumId, data.album);
    if (res.status === "ok") {
      toggleFavAlbumLocal(albumId, data.album, res.data);
      toast.success(res.data ? "Album ajouté aux favoris" : "Album retiré des favoris");
    } else {
      toast.error(res.error);
    }
  };

  return (
    <div className="flex flex-1 min-h-0 flex-col space-y-6 pb-4">
      <div className="flex flex-col gap-6 md:flex-row md:items-end">
        <CoverImage
          coverId={data.album.cover_id}
          alt={data.album.title}
          size={640}
          className="h-48 w-48 shadow-2xl"
          rounded="2xl"
        />
        <div className="min-w-0 flex-1 space-y-2">
          <div className="flex items-center gap-2">
            <Badge variant={tier === "MAX" ? "max" : tier === "HIGH" ? "high" : "low"}>
              {tier}
            </Badge>
            {data.album.release_date && (
              <span className="text-xs text-white/45 tabular-nums">
                {data.album.release_date.slice(0, 4)}
              </span>
            )}
          </div>
          <h1 className="text-3xl font-bold tracking-tight text-white">{data.album.title}</h1>
          <div className="flex flex-wrap items-center gap-2 text-sm text-white/60">
            <button
              type="button"
              onClick={() =>
                data.album.artist_id &&
                navigate({ name: "artist", artistId: data.album.artist_id })
              }
              className="font-medium text-white hover:underline cursor-pointer"
            >
              {data.album.artist_name}
            </button>
            <span className="text-white/25">•</span>
            <span className="tabular-nums">{data.tracks.length} titres</span>
            <span className="text-white/25">•</span>
            <span className="tabular-nums">{formatSecs(totalSecs)}</span>
          </div>

          <div className="flex items-center gap-3 pt-2">
            <Button onClick={() => void commands.playTracks(data.tracks, 0)}>
              <Play className="h-4 w-4 fill-current" />
              Lire
            </Button>
            <Button
              variant="secondary"
              onClick={async () => {
                await commands.setShuffle(true);
                await commands.playTracks(data.tracks, 0);
              }}
            >
              <Shuffle className="h-4 w-4" />
              Aléatoire
            </Button>
            <Button variant="outline" size="icon" onClick={handleToggleFav} aria-label="Favori album">
              <Heart
                className={`h-4 w-4 ${isFav ? "text-[#FF2D55]" : ""}`}
                fill={isFav ? "currentColor" : "none"}
              />
            </Button>
          </div>
        </div>
      </div>

      <VirtualTrackList tracks={data.tracks} showCover={false} showAlbum={false} />
    </div>
  );
}
