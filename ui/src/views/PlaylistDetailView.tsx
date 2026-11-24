import { Play, Shuffle, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { type PlaylistDetailData, type TrackSummary, commands } from "@/bindings";
import { CoverImage } from "@/components/media/CoverImage";
import { VirtualTrackList } from "@/components/media/VirtualTrackList";
import { Button } from "@/components/ui/button";
import { useLibraryStore } from "@/stores/libraryStore";
import { useUiStore } from "@/stores/uiStore";

interface PlaylistDetailViewProps {
  playlistUuid: string;
}

export function PlaylistDetailView({ playlistUuid }: PlaylistDetailViewProps) {
  const [data, setData] = useState<PlaylistDetailData | null>(null);
  const [loading, setLoading] = useState(true);
  const removePlaylistLocal = useLibraryStore((s) => s.removePlaylistLocal);
  const navigate = useUiStore((s) => s.navigate);

  const load = useCallback(async () => {
    setLoading(true);
    const res = await commands.getPlaylistTracks(playlistUuid);
    setLoading(false);
    if (res.status === "ok") setData(res.data);
    else toast.error(res.error);
  }, [playlistUuid]);

  useEffect(() => {
    void load();
  }, [load]);

  if (loading || !data) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-white/45">
        {loading ? "Chargement de la playlist…" : "Playlist introuvable."}
      </div>
    );
  }

  const handleRemoveTrack = async (displayIdx: number, track: TrackSummary) => {
    const tidalIndex = data.tidal_indices[displayIdx] ?? displayIdx;
    const res = await commands.removeTrackFromPlaylist(playlistUuid, tidalIndex, track.id);
    if (res.status === "ok") {
      toast.success("Titre retiré de la playlist");
      void load();
    } else {
      toast.error(res.error);
    }
  };

  const handleDeletePlaylist = async () => {
    const res = await commands.deletePlaylist(playlistUuid);
    if (res.status === "ok") {
      removePlaylistLocal(playlistUuid);
      toast.success("Playlist supprimée");
      navigate({ name: "collection", initialTab: "playlists" });
    } else {
      toast.error(res.error);
    }
  };

  return (
    <div className="flex flex-1 min-h-0 flex-col space-y-6 pb-4">
      <div className="flex flex-col gap-6 md:flex-row md:items-end">
        <CoverImage
          coverId={data.playlist.image_id ?? data.tracks[0]?.cover_id}
          alt={data.playlist.title}
          size={640}
          className="h-44 w-44 shadow-2xl"
          rounded="2xl"
        />
        <div className="min-w-0 flex-1 space-y-2">
          <p className="text-xs font-semibold uppercase tracking-widest text-white/45">Playlist</p>
          <h1 className="text-3xl font-bold tracking-tight text-white">{data.playlist.title}</h1>
          {data.playlist.description && (
            <p className="text-xs text-white/55 max-w-2xl">{data.playlist.description}</p>
          )}
          <p className="text-xs text-white/45 tabular-nums">
            {data.playlist.creator ? `${data.playlist.creator} • ` : ""}
            {data.tracks.length} titres
          </p>

          <div className="flex items-center gap-3 pt-2">
            <Button
              disabled={data.tracks.length === 0}
              onClick={() => void commands.playTracks(data.tracks, 0)}
            >
              <Play className="h-4 w-4 fill-current" />
              Lire
            </Button>
            <Button
              variant="secondary"
              disabled={data.tracks.length === 0}
              onClick={async () => {
                await commands.setShuffle(true);
                await commands.playTracks(data.tracks, 0);
              }}
            >
              <Shuffle className="h-4 w-4" />
              Aléatoire
            </Button>
            <Button
              variant="destructive"
              size="sm"
              onClick={() => void handleDeletePlaylist()}
            >
              <Trash2 className="h-3.5 w-3.5" />
              Supprimer
            </Button>
          </div>
        </div>
      </div>

      <VirtualTrackList tracks={data.tracks} onRemoveFromPlaylist={handleRemoveTrack} />
    </div>
  );
}
