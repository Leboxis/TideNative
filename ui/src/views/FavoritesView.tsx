import { Heart, Play, RefreshCw, Search, Shuffle, X } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { LoginGate } from "@/components/auth/LoginGate";
import { VirtualTrackList } from "@/components/media/VirtualTrackList";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { formatSecs } from "@/lib/utils";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";

type TrackSortOrder = "Default" | "TitleAsc" | "ArtistAsc" | "AlbumAsc" | "DurationDesc";

const SORT_LABELS: Record<TrackSortOrder, string> = {
  Default: "Ordre d'ajout",
  TitleAsc: "Titre (A–Z)",
  ArtistAsc: "Artiste (A–Z)",
  AlbumAsc: "Album (A–Z)",
  DurationDesc: "Durée",
};

export function FavoritesView() {
  const authenticated = usePlaybackStore((s) => s.authenticated);
  const favorites = useLibraryStore((s) => s.favorites);
  const [filter, setFilter] = useState("");
  const [sort, setSort] = useState<TrackSortOrder>("Default");
  const [syncing, setSyncing] = useState(false);

  const displayedFavorites = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const list = q
      ? favorites.filter(
          (t) =>
            t.title.toLowerCase().includes(q) ||
            t.artist_name.toLowerCase().includes(q) ||
            t.album_title.toLowerCase().includes(q),
        )
      : [...favorites];

    switch (sort) {
      case "TitleAsc":
        list.sort((a, b) => a.title.localeCompare(b.title));
        break;
      case "ArtistAsc":
        list.sort((a, b) => a.artist_name.localeCompare(b.artist_name));
        break;
      case "AlbumAsc":
        list.sort((a, b) => a.album_title.localeCompare(b.album_title));
        break;
      case "DurationDesc":
        list.sort((a, b) => b.duration_secs - a.duration_secs);
        break;
      default:
        break;
    }
    return list;
  }, [favorites, filter, sort]);

  const totalSecs = useMemo(
    () => displayedFavorites.reduce((acc, t) => acc + t.duration_secs, 0),
    [displayedFavorites],
  );

  if (!authenticated && favorites.length === 0) {
    return <LoginGate />;
  }

  const handlePlayAll = () => {
    if (displayedFavorites.length === 0) return;
    void commands.playTracks(displayedFavorites, 0);
  };

  const handleShufflePlay = async () => {
    if (displayedFavorites.length === 0) return;
    await commands.setShuffle(true);
    const randomIdx = Math.floor(Math.random() * displayedFavorites.length);
    void commands.playTracks(displayedFavorites, randomIdx);
  };

  const handleSync = async () => {
    setSyncing(true);
    const res = await commands.refreshLibrary();
    setSyncing(false);
    if (res.status === "ok") {
      toast.success("Favoris TIDAL synchronisés");
    } else {
      toast.error(res.error);
    }
  };

  return (
    <div className="flex flex-1 min-h-0 flex-col space-y-5 pb-4">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex items-center gap-3.5">
          <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-2xl border border-[#FF2D55]/30 bg-[#FF2D55]/15 text-[#FF2D55]">
            <Heart className="h-5 w-5 fill-[#FF2D55] text-[#FF2D55]" />
          </div>
          <div>
            <h1 className="text-2xl font-semibold tracking-tight text-white">Favoris</h1>
            <p className="text-xs text-white/45 mt-0.5 tabular-nums">
              {displayedFavorites.length} titre{displayedFavorites.length > 1 ? "s" : ""} aimé
              {displayedFavorites.length > 1 ? "s" : ""}
              {totalSecs > 0 ? ` • ${formatSecs(totalSecs)}` : ""}
            </p>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <div className="relative w-44 focus-within:w-60 transition-all duration-300 ease-out">
            <Search className="pointer-events-none absolute left-3 top-2.5 h-3.5 w-3.5 text-white/35" />
            <Input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="Filtrer vos favoris…"
              className="pl-8 pr-8 h-8 text-xs focus-visible:border-[#33F3FF]/50 focus-visible:shadow-[0_0_16px_rgba(51,243,255,0.16)] transition-all duration-300"
            />
            {filter && (
              <button
                type="button"
                aria-label="Effacer le filtre"
                onClick={() => setFilter("")}
                className="absolute right-2.5 top-2 text-white/40 hover:text-white cursor-pointer"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            )}
          </div>

          <Select value={sort} onValueChange={(v) => setSort(v as TrackSortOrder)}>
            <SelectTrigger className="w-40 h-8 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {(Object.keys(SORT_LABELS) as TrackSortOrder[]).map((k) => (
                <SelectItem key={k} value={k}>
                  {SORT_LABELS[k]}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          {displayedFavorites.length > 0 && (
            <>
              <Button size="sm" onClick={handlePlayAll}>
                <Play className="h-3.5 w-3.5 fill-current" />
                Tout lire
              </Button>
              <Button variant="secondary" size="sm" onClick={() => void handleShufflePlay()}>
                <Shuffle className="h-3.5 w-3.5" />
                Aléatoire
              </Button>
            </>
          )}

          <Button
            variant="secondary"
            size="sm"
            onClick={() => void handleSync()}
            disabled={syncing}
            title="Synchroniser les favoris TIDAL"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${syncing ? "animate-spin" : ""}`} />
          </Button>
        </div>
      </div>

      <div className="flex flex-1 min-h-0 flex-col">
        <VirtualTrackList
          tracks={displayedFavorites}
          emptyLabel={
            filter.trim()
              ? "Aucun titre aimé ne correspond à votre filtre."
              : "Aucun titre aimé dans votre bibliothèque TIDAL."
          }
        />
      </div>
    </div>
  );
}
