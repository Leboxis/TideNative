import {
  CheckSquare,
  Heart,
  ListMusic,
  ListPlus,
  Play,
  RefreshCw,
  Search,
  Shuffle,
  X,
} from "lucide-react";
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
import { type TidalQualityTier, formatSecs, qualityTierFromApi } from "@/lib/utils";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";

type TrackSortOrder = "Default" | "TitleAsc" | "ArtistAsc" | "AlbumAsc" | "DurationDesc";
type QualityFilter = "ALL" | TidalQualityTier;

const SORT_LABELS: Record<TrackSortOrder, string> = {
  Default: "Ordre d'ajout",
  TitleAsc: "Titre (A–Z)",
  ArtistAsc: "Artiste (A–Z)",
  AlbumAsc: "Album (A–Z)",
  DurationDesc: "Durée",
};

const QUALITY_FILTER_STYLES: Record<QualityFilter, { active: string; idle: string }> = {
  ALL: {
    active: "bg-white text-[#050507] border-white font-semibold",
    idle: "bg-white/[0.04] text-white/65 border-white/10 hover:text-white hover:border-white/20",
  },
  MAX: {
    active: "bg-[#FFD432]/20 text-[#FFD432] border-[#FFD432]/45 font-semibold",
    idle: "bg-white/[0.04] text-[#FFD432]/70 border-white/10 hover:border-[#FFD432]/30 hover:text-[#FFD432]",
  },
  HIGH: {
    active: "bg-[#33F3FF]/18 text-[#33F3FF] border-[#33F3FF]/45 font-semibold",
    idle: "bg-white/[0.04] text-[#33F3FF]/70 border-white/10 hover:border-[#33F3FF]/30 hover:text-[#33F3FF]",
  },
  LOW: {
    active: "bg-white/15 text-[#A8A8B3] border-white/30 font-semibold",
    idle: "bg-white/[0.04] text-[#A8A8B3]/70 border-white/10 hover:border-white/25 hover:text-[#A8A8B3]",
  },
};

export function FavoritesView() {
  const authenticated = usePlaybackStore((s) => s.authenticated);
  const favorites = useLibraryStore((s) => s.favorites);
  const openAddToPlaylistMany = useUiStore((s) => s.openAddToPlaylistMany);
  const [filter, setFilter] = useState("");
  const [qualityFilter, setQualityFilter] = useState<QualityFilter>("ALL");
  const [sort, setSort] = useState<TrackSortOrder>("Default");
  const [syncing, setSyncing] = useState(false);
  const [selectionMode, setSelectionMode] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(() => new Set());

  const qualityCounts = useMemo(() => {
    const counts: Record<QualityFilter, number> = {
      ALL: favorites.length,
      MAX: 0,
      HIGH: 0,
      LOW: 0,
    };
    for (const track of favorites) {
      counts[qualityTierFromApi(track.audio_quality)] += 1;
    }
    return counts;
  }, [favorites]);

  const displayedFavorites = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const list = favorites.filter((t) => {
      if (qualityFilter !== "ALL" && qualityTierFromApi(t.audio_quality) !== qualityFilter) {
        return false;
      }
      if (!q) return true;
      return (
        t.title.toLowerCase().includes(q) ||
        t.artist_name.toLowerCase().includes(q) ||
        t.album_title.toLowerCase().includes(q)
      );
    });

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
  }, [favorites, filter, qualityFilter, sort]);

  const selectedTracks = useMemo(
    () => displayedFavorites.filter((t) => selectedIds.has(t.id)),
    [displayedFavorites, selectedIds],
  );

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

  const toggleSelectTrack = (trackId: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(trackId)) next.delete(trackId);
      else next.add(trackId);
      return next;
    });
  };

  const toggleSelectAllDisplayed = () => {
    const allDisplayedSelected =
      displayedFavorites.length > 0 && displayedFavorites.every((t) => selectedIds.has(t.id));
    if (allDisplayedSelected) {
      setSelectedIds(new Set());
    } else {
      setSelectedIds(new Set(displayedFavorites.map((t) => t.id)));
    }
  };

  const exitSelectionMode = () => {
    setSelectionMode(false);
    setSelectedIds(new Set());
  };

  const handlePlaySelection = () => {
    if (selectedTracks.length === 0) return;
    void commands.playTracks(selectedTracks, 0);
    toast.success(`Lecture de ${selectedTracks.length} titre${selectedTracks.length > 1 ? "s" : ""}`);
  };

  const handleQueueSelection = async () => {
    if (selectedTracks.length === 0) return;
    let queued = 0;
    for (const track of selectedTracks) {
      const res = await commands.queueAdd(track);
      if (res.status === "ok") queued += 1;
    }
    if (queued > 0) {
      toast.success(`${queued} titre${queued > 1 ? "s" : ""} ajouté${queued > 1 ? "s" : ""} à la file`);
      exitSelectionMode();
    }
  };

  const handleAddSelectionToPlaylist = () => {
    if (selectedTracks.length === 0) return;
    openAddToPlaylistMany(selectedTracks);
  };

  return (
    <div className="flex flex-1 min-h-0 flex-col space-y-4 pb-4">
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
              <Button
                variant={selectionMode ? "default" : "outline"}
                size="sm"
                onClick={() => (selectionMode ? exitSelectionMode() : setSelectionMode(true))}
              >
                <CheckSquare className="h-3.5 w-3.5" />
                {selectionMode ? "Terminer" : "Sélectionner"}
              </Button>
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

      {/* Quick quality filters & multi-select bulk action bar */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div
          role="group"
          aria-label="Filtrer par qualité audio"
          className="flex flex-wrap items-center gap-1.5"
        >
          {(["ALL", "MAX", "HIGH", "LOW"] as QualityFilter[]).map((tier) => {
            const count = qualityCounts[tier];
            if (tier !== "ALL" && count === 0) return null;
            const active = qualityFilter === tier;
            const styles = QUALITY_FILTER_STYLES[tier];
            return (
              <button
                key={tier}
                type="button"
                aria-pressed={active}
                onClick={() => setQualityFilter(tier)}
                className={`inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-[11px] transition-colors cursor-pointer tabular-nums ${
                  active ? styles.active : styles.idle
                }`}
              >
                <span>{tier === "ALL" ? "Toutes qualités" : tier}</span>
                <span className="opacity-70">{count}</span>
              </button>
            );
          })}
        </div>

        {selectionMode && (
          <div className="flex flex-wrap items-center gap-2 rounded-xl border border-[#33F3FF]/25 bg-[#33F3FF]/[0.07] px-3 py-1.5">
            <span className="text-xs font-medium text-white tabular-nums">
              {selectedTracks.length} sélectionné{selectedTracks.length > 1 ? "s" : ""}
            </span>
            <Button
              variant="ghost"
              size="sm"
              className="h-7 px-2 text-xs"
              onClick={toggleSelectAllDisplayed}
            >
              {selectedTracks.length === displayedFavorites.length && displayedFavorites.length > 0
                ? "Tout désélectionner"
                : "Tout sélectionner"}
            </Button>
            <Button
              size="sm"
              className="h-7 px-2.5 text-xs"
              disabled={selectedTracks.length === 0}
              onClick={handlePlaySelection}
            >
              <Play className="h-3 w-3 fill-current" />
              Lire
            </Button>
            <Button
              variant="secondary"
              size="sm"
              className="h-7 px-2.5 text-xs"
              disabled={selectedTracks.length === 0}
              onClick={() => void handleQueueSelection()}
            >
              <ListPlus className="h-3.5 w-3.5" />
              À la file
            </Button>
            <Button
              variant="secondary"
              size="sm"
              className="h-7 px-2.5 text-xs"
              disabled={selectedTracks.length === 0}
              onClick={handleAddSelectionToPlaylist}
            >
              <ListMusic className="h-3.5 w-3.5" />
              Playlist
            </Button>
          </div>
        )}
      </div>

      <div className="flex flex-1 min-h-0 flex-col">
        <VirtualTrackList
          tracks={displayedFavorites}
          selectable={selectionMode}
          selectedIds={selectedIds}
          onToggleSelect={toggleSelectTrack}
          onToggleSelectAll={toggleSelectAllDisplayed}
          emptyLabel={
            filter.trim() || qualityFilter !== "ALL"
              ? "Aucun titre aimé ne correspond à vos filtres."
              : "Aucun titre aimé dans votre bibliothèque TIDAL."
          }
        />
      </div>
    </div>
  );
}
