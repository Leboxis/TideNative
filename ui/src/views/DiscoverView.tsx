import { Play, RefreshCw, Sparkles } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { LoginGate } from "@/components/auth/LoginGate";
import { CoverImage } from "@/components/media/CoverImage";
import { VirtualTrackList } from "@/components/media/VirtualTrackList";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";

export function DiscoverView() {
  const authenticated = usePlaybackStore((s) => s.authenticated);
  const userMixes = useLibraryStore((s) => s.userMixes);
  const discoverTracks = useLibraryStore((s) => s.discoverTracks);
  const recentTracks = useLibraryStore((s) => s.recentTracks);
  const favorites = useLibraryStore((s) => s.favorites);
  const navigate = useUiStore((s) => s.navigate);
  const [syncing, setSyncing] = useState(false);

  if (!authenticated && favorites.length === 0 && userMixes.length === 0) {
    return <LoginGate />;
  }

  const featuredTracks = discoverTracks.length > 0 ? discoverTracks : favorites.slice(0, 30);
  const heroTrack = featuredTracks[0] ?? recentTracks[0] ?? null;

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

  return (
    <div className="space-y-8 pb-8">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-white">Écouter</h1>
          <p className="text-xs text-white/45 mt-0.5">
            Vos mix quotidiens personnalisés et sélections Hi-Res Master
          </p>
        </div>
        <Button variant="secondary" size="sm" onClick={handleSync} disabled={syncing}>
          <RefreshCw className={`h-3.5 w-3.5 ${syncing ? "animate-spin" : ""}`} />
          Synchroniser
        </Button>
      </div>

      {/* Hero Cinema Card */}
      {heroTrack && (
        <section className="card-oled relative overflow-hidden rounded-3xl p-6">
          <div className="flex flex-col gap-6 md:flex-row md:items-center">
            <CoverImage
              coverId={heroTrack.cover_id}
              alt={heroTrack.title}
              size={640}
              className="h-36 w-36 shadow-2xl"
              rounded="2xl"
            />
            <div className="min-w-0 flex-1 space-y-2">
              <div className="flex items-center gap-2">
                <Badge variant="max">HI-RES MASTER</Badge>
                <span className="inline-flex items-center gap-1 text-[11px] text-white/50">
                  <Sparkles className="h-3 w-3 text-[#33F3FF]" />
                  Sélection Bit-Perfect
                </span>
              </div>
              <h2 className="truncate text-2xl font-bold tracking-tight text-white">
                {heroTrack.title}
              </h2>
              <p className="truncate text-sm text-white/60">
                {heroTrack.artist_name} — {heroTrack.album_title}
              </p>
              <div className="flex items-center gap-3 pt-2">
                <Button onClick={() => void commands.playTracks(featuredTracks, 0)}>
                  <Play className="h-4 w-4 fill-current" />
                  Lancer l&apos;écoute
                </Button>
                {heroTrack.album_id && (
                  <Button
                    variant="secondary"
                    onClick={() => navigate({ name: "album", albumId: heroTrack.album_id! })}
                  >
                    Ouvrir l&apos;album
                  </Button>
                )}
              </div>
            </div>
          </div>
        </section>
      )}

      {/* Personalized Mixes */}
      {userMixes.length > 0 && (
        <section className="space-y-3">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-white/50">
            Vos Mix Personnalisés
          </h2>
          <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
            {userMixes.map((mix) => (
              <button
                key={mix.id}
                type="button"
                onClick={() =>
                  navigate({
                    name: "mix",
                    mixId: mix.id,
                    title: mix.title,
                    subtitle: mix.subtitle,
                  })
                }
                className="group card-oled flex flex-col rounded-2xl p-3 text-left transition-[transform,border-color] duration-150 hover:-translate-y-0.5 hover:border-white/20 cursor-pointer"
              >
                <div className="relative aspect-square w-full overflow-hidden rounded-xl mb-3">
                  <CoverImage
                    coverId={mix.image_id ?? mix.tracks[0]?.cover_id}
                    alt={mix.title}
                    size={320}
                    className="h-full w-full"
                    rounded="xl"
                  />
                  <span className="absolute bottom-2 right-2 flex h-9 w-9 items-center justify-center rounded-full bg-white text-[#050507] opacity-0 shadow-xl transition-opacity duration-150 group-hover:opacity-100">
                    <Play className="h-4 w-4 fill-current translate-x-[1px]" />
                  </span>
                </div>
                <p className="truncate text-sm font-semibold text-white">{mix.title}</p>
                <p className="truncate text-xs text-white/45 mt-0.5">{mix.subtitle}</p>
              </button>
            ))}
          </div>
        </section>
      )}

      {/* Recent Tracks */}
      {recentTracks.length > 0 && (
        <section className="space-y-3">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-white/50">
            Écoutés récemment
          </h2>
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
            {recentTracks.slice(0, 10).map((track, idx) => (
              <button
                key={`recent-${track.id}-${idx}`}
                type="button"
                onClick={() => void commands.playTracks(recentTracks, idx)}
                className="group card-oled flex items-center gap-3 rounded-xl p-2 text-left hover:border-white/20 transition-colors cursor-pointer"
              >
                <CoverImage
                  coverId={track.cover_id}
                  alt={track.title}
                  size={160}
                  className="h-11 w-11"
                  rounded="md"
                />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-xs font-semibold text-white">{track.title}</p>
                  <p className="truncate text-[11px] text-white/45">{track.artist_name}</p>
                </div>
              </button>
            ))}
          </div>
        </section>
      )}

      {/* Featured Hi-Res Tracks */}
      <section className="flex flex-col space-y-3">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-white/50">
            Sélection Hi-Res
          </h2>
          <span className="text-xs text-white/35 tabular-nums">{featuredTracks.length} titres</span>
        </div>
        <div className="h-[420px] flex flex-col">
          <VirtualTrackList tracks={featuredTracks} />
        </div>
      </section>
    </div>
  );
}
