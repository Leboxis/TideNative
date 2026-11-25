import { Heart, Play, Radio, Shuffle } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { type AlbumDetailData, commands } from "@/bindings";
import { CoverImage } from "@/components/media/CoverImage";
import { DetailHeaderSurface } from "@/components/media/DetailHeaderSurface";
import { MediaCard } from "@/components/media/MediaCard";
import { VirtualTrackList } from "@/components/media/VirtualTrackList";
import { Button } from "@/components/ui/button";
import { DetailHeaderSkeleton, TrackListSkeleton } from "@/components/ui/skeleton";
import { quickPlayAlbum } from "@/lib/quickPlay";
import { formatSecs } from "@/lib/utils";
import { telemetryRuntime } from "@/lib/telemetry";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
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

  if (loading) {
    return (
      <div className="space-y-8 pb-8">
        <DetailHeaderSkeleton />
        <TrackListSkeleton rows={7} />
      </div>
    );
  }

  if (!data) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 text-sm text-white/60">
        <p>Album introuvable.</p>
        <Button variant="secondary" size="sm" onClick={() => navigate({ name: "explore" })}>
          Explorer le catalogue
        </Button>
      </div>
    );
  }

  const totalSecs = data.tracks.reduce((acc, t) => acc + t.duration_secs, 0);
  const similarAlbums = data.similar_albums ?? [];

  const handlePlayAlbum = async () => {
    if (data.tracks.length === 0) return;
    if (usePlaybackStore.getState().preferences.shuffle_enabled) {
      await commands.setShuffle(false);
    }
    telemetryRuntime.seedPosition(0, data.tracks[0].duration_secs * 1000);
    const res = await commands.playTracks(data.tracks, 0);
    if (res.status === "error") {
      toast.error(res.error);
    }
  };

  const handleShuffleAlbum = async () => {
    if (data.tracks.length === 0) return;
    const randomIdx = Math.floor(Math.random() * data.tracks.length);
    telemetryRuntime.seedPosition(0, data.tracks[randomIdx].duration_secs * 1000);
    const res = await commands.playTracks(data.tracks, randomIdx);
    if (res.status === "error") {
      toast.error(res.error);
      return;
    }
    if (!usePlaybackStore.getState().preferences.shuffle_enabled) {
      await commands.setShuffle(true);
    }
  };

  const handleToggleFav = async () => {
    const res = await commands.toggleFavoriteAlbum(albumId, data.album);
    if (res.status === "ok") {
      toggleFavAlbumLocal(albumId, data.album, res.data);
      toast.success(res.data ? "Album ajouté aux favoris" : "Album retiré des favoris");
    } else {
      toast.error(res.error);
    }
  };

  const handleAlbumRadio = async () => {
    const res = await commands.playAlbumRadio(albumId);
    if (res.status === "ok") {
      toast.success(`Radio lancée pour « ${data.album.title} »`);
    } else {
      toast.error(res.error);
    }
  };

  return (
    <div
      className={
        similarAlbums.length > 0
          ? "space-y-8 pb-8"
          : "flex flex-1 min-h-0 flex-col space-y-6 pb-4"
      }
    >
      <DetailHeaderSurface
        coverId={data.album.cover_id}
        className="flex flex-col gap-6 md:flex-row md:items-end"
      >
        <CoverImage
          coverId={data.album.cover_id}
          alt={data.album.title}
          size={640}
          className="h-48 w-48 shadow-2xl"
          rounded="2xl"
        />
        <div className="min-w-0 flex-1 space-y-2">
          <div className="flex items-center gap-2">
            <span className="text-xs font-semibold uppercase tracking-widest text-white/45">
              Album
            </span>
            {data.album.release_date && (
              <>
                <span className="text-white/25">•</span>
                <span className="text-xs text-white/45 tabular-nums">
                  {data.album.release_date.slice(0, 4)}
                </span>
              </>
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

          <div className="flex flex-wrap items-center gap-3 pt-2">
            <Button disabled={data.tracks.length === 0} onClick={() => void handlePlayAlbum()}>
              <Play className="h-4 w-4 fill-current" />
              Lire
            </Button>
            <Button
              variant="secondary"
              disabled={data.tracks.length === 0}
              onClick={() => void handleShuffleAlbum()}
            >
              <Shuffle className="h-4 w-4" />
              Aléatoire
            </Button>
            <Button variant="secondary" onClick={() => void handleAlbumRadio()}>
              <Radio className="h-4 w-4" />
              Radio album
            </Button>
            <Button variant="outline" size="icon" onClick={handleToggleFav} aria-label="Favori album">
              <Heart
                className={`h-4 w-4 ${isFav ? "text-[#FF2D55]" : ""}`}
                fill={isFav ? "currentColor" : "none"}
              />
            </Button>
          </div>
        </div>
      </DetailHeaderSurface>

      {similarAlbums.length > 0 ? (
        <>
          <VirtualTrackList tracks={data.tracks} showCover={false} showAlbum={false} embedded />

          <section className="space-y-3">
            <h2 className="text-sm font-semibold uppercase tracking-wider text-white/60">
              Albums similaires
            </h2>
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
              {similarAlbums.map((album) => (
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
        </>
      ) : (
        <VirtualTrackList tracks={data.tracks} showCover={false} showAlbum={false} />
      )}
    </div>
  );
}
