import { Play, Shuffle } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { type TrackSummary, commands } from "@/bindings";
import { CoverImage } from "@/components/media/CoverImage";
import { DetailHeaderSurface } from "@/components/media/DetailHeaderSurface";
import { VirtualTrackList } from "@/components/media/VirtualTrackList";
import { Button } from "@/components/ui/button";
import { TrackListSkeleton } from "@/components/ui/skeleton";
import { useLibraryStore } from "@/stores/libraryStore";

interface MixDetailViewProps {
  mixId: string;
  title: string;
  subtitle?: string;
}

export function MixDetailView({ mixId, title, subtitle }: MixDetailViewProps) {
  const existingMix = useLibraryStore((s) => s.userMixes.find((m) => m.id === mixId));
  const [tracks, setTracks] = useState<TrackSummary[]>(existingMix?.tracks ?? []);
  const [loading, setLoading] = useState((existingMix?.tracks.length ?? 0) === 0);

  useEffect(() => {
    if (existingMix && existingMix.tracks.length > 0) {
      setTracks(existingMix.tracks);
      setLoading(false);
      return;
    }
    let active = true;
    setLoading(true);
    void commands.getMixTracks(mixId).then((res) => {
      if (!active) return;
      setLoading(false);
      if (res.status === "ok") setTracks(res.data);
      else toast.error(res.error);
    });
    return () => {
      active = false;
    };
  }, [mixId, existingMix]);

  return (
    <div className="flex flex-1 min-h-0 flex-col space-y-6 pb-4">
      <DetailHeaderSurface
        coverId={existingMix?.image_id ?? tracks[0]?.cover_id}
        className="flex flex-col gap-6 sm:flex-row sm:items-end"
      >
        <CoverImage
          coverId={existingMix?.image_id ?? tracks[0]?.cover_id}
          alt={title}
          size={640}
          className="h-44 w-44 shadow-2xl"
          rounded="2xl"
        />
        <div className="min-w-0 flex-1 space-y-2">
          <p className="text-xs font-semibold uppercase tracking-widest text-[#33F3FF]">
            Mix personnalisé
          </p>
          <h1 className="text-3xl font-bold tracking-tight text-white">{title}</h1>
          {subtitle && <p className="text-xs text-white/55">{subtitle}</p>}
          <div className="flex items-center gap-3 pt-2">
            <Button disabled={tracks.length === 0} onClick={() => void commands.playTracks(tracks, 0)}>
              <Play className="h-4 w-4 fill-current" />
              Lire le mix
            </Button>
            <Button
              variant="secondary"
              disabled={tracks.length === 0}
              onClick={async () => {
                await commands.setShuffle(true);
                await commands.playTracks(tracks, 0);
              }}
            >
              <Shuffle className="h-4 w-4" />
              Aléatoire
            </Button>
          </div>
        </div>
      </DetailHeaderSurface>

      {loading ? (
        <TrackListSkeleton rows={7} />
      ) : (
        <VirtualTrackList tracks={tracks} />
      )}
    </div>
  );
}
