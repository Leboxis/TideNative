import { useVirtualizer } from "@tanstack/react-virtual";
import { Check, Heart, MoreHorizontal, Play } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { type TrackSummary, commands } from "@/bindings";
import { CoverImage } from "@/components/media/CoverImage";
import { DolbyAudioLogo } from "@/components/media/DolbyAudioLogo";
import { TrackContextMenu } from "@/components/media/TrackContextMenu";
import { AudioEqualizerBars } from "@/components/player/AudioEqualizerBars";
import { Badge } from "@/components/ui/badge";
import {
  type TrackRowColorPalette,
  coverUrl,
  extractTrackRowColors,
  getCachedTrackRowColors,
  hexToRgba,
} from "@/lib/covers";
import { telemetryRuntime } from "@/lib/telemetry";
import { formatSecs, hasDolbyAudio, qualityTierFromApi } from "@/lib/utils";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";

interface VirtualTrackListProps {
  tracks: TrackSummary[];
  showCover?: boolean;
  showAlbum?: boolean;
  showIndex?: boolean;
  embedded?: boolean;
  selectable?: boolean;
  selectedIds?: ReadonlySet<number>;
  onToggleSelect?: (trackId: number) => void;
  onToggleSelectAll?: () => void;
  onPlayTrack?: (index: number) => void;
  onRemoveFromPlaylist?: (index: number, track: TrackSummary) => void;
  emptyLabel?: string;
}

const ROW_HEIGHT = 56;

const FALLBACK_ROW_PALETTE: TrackRowColorPalette = {
  colors: ["#33F3FF", "#6366F1", "#EC4899", "#F59E0B"],
  titleColor: "#33F3FF",
};

export function VirtualTrackList({
  tracks,
  showCover = true,
  showAlbum = true,
  showIndex = true,
  embedded = false,
  selectable = false,
  selectedIds,
  onToggleSelect,
  onToggleSelectAll,
  onPlayTrack,
  onRemoveFromPlaylist,
  emptyLabel = "Aucun titre à afficher.",
}: VirtualTrackListProps) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const currentTrack = usePlaybackStore((s) => s.currentTrack);
  const currentTrackId = currentTrack?.id ?? null;
  const playbackState = usePlaybackStore((s) => s.playbackState);
  const currentPalette = usePlaybackStore((s) => s.currentPalette);
  const favoriteIds = useLibraryStore((s) => s.favoriteIds);
  const toggleFavLocal = useLibraryStore((s) => s.toggleFavoriteTrackLocal);
  const navigate = useUiStore((s) => s.navigate);
  const openAddToPlaylist = useUiStore((s) => s.openAddToPlaylist);

  const [rowPalette, setRowPalette] = useState<TrackRowColorPalette>(() => {
    const cached = getCachedTrackRowColors(currentTrack?.cover_id);
    if (cached) return cached;
    return {
      colors: [
        currentPalette.accent || "#33F3FF",
        currentPalette.primary || "#6366F1",
        currentPalette.secondary || "#EC4899",
        currentPalette.accent || "#F59E0B",
      ],
      titleColor: currentPalette.accent || "#33F3FF",
    };
  });

  useEffect(() => {
    const coverId = currentTrack?.cover_id;
    if (!coverId) {
      setRowPalette(FALLBACK_ROW_PALETTE);
      return;
    }
    if (currentPalette.row_colors && currentPalette.row_colors.length === 4) {
      setRowPalette({
        colors: currentPalette.row_colors,
        titleColor: currentPalette.accent || currentPalette.row_colors[0] || "#33F3FF",
      });
    }
    const cached = getCachedTrackRowColors(coverId);
    if (cached) {
      setRowPalette(cached);
      return;
    }
    let alive = true;
    void extractTrackRowColors(coverId).then((extracted) => {
      if (alive && extracted) {
        setRowPalette(extracted);
      }
    });
    return () => {
      alive = false;
    };
  }, [
    currentTrack?.id,
    currentTrack?.cover_id,
    currentPalette.accent,
    currentPalette.row_colors,
  ]);

  const [c1, c2, c3, c4] = rowPalette.colors;
  const titleColor = rowPalette.titleColor;

  const virtualizer = useVirtualizer({
    count: tracks.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
    initialRect: { width: 1000, height: 600 },
    ...(typeof ResizeObserver === "undefined"
      ? {
          observeElementRect: (_inst, cb) => {
            cb({ width: 1000, height: 600 });
            return () => {};
          },
        }
      : {}),
  });

  if (tracks.length === 0) {
    return (
      <div className="flex h-40 items-center justify-center rounded-2xl border border-white/[0.06] bg-white/[0.02] text-sm text-white/60">
        {emptyLabel}
      </div>
    );
  }

  const handlePlay = async (idx: number) => {
    const selected = tracks[idx];
    if (selected) {
      telemetryRuntime.seedPosition(0, selected.duration_secs * 1000);
    }
    if (selected?.cover_id) {
      const cached = getCachedTrackRowColors(selected.cover_id);
      if (cached) {
        setRowPalette(cached);
      } else {
        void extractTrackRowColors(selected.cover_id).then((extracted) => {
          if (extracted) {
            setRowPalette(extracted);
          }
        });
      }
    }
    if (onPlayTrack) {
      onPlayTrack(idx);
    } else {
      const res = await commands.playTracks(tracks, idx);
      if (res.status === "error") {
        toast.error(res.error);
      }
    }
  };

  const gridColsClass = showAlbum
    ? "grid-cols-[40px_1fr_84px_104px] md:grid-cols-[40px_1fr_200px_84px_104px]"
    : "grid-cols-[40px_1fr_84px_104px]";

  const allSelected =
    selectable && tracks.length > 0 && selectedIds !== undefined
      ? tracks.every((t) => selectedIds.has(t.id))
      : false;

  const renderTrackRow = (track: TrackSummary, index: number) => {
    const isActive = currentTrackId === track.id;
    const isPlaying = isActive && playbackState === "Playing";
    const isFav = favoriteIds.has(track.id);
    const isSelected = Boolean(selectable && selectedIds?.has(track.id));
    const tier = qualityTierFromApi(track.audio_quality);
    const activeCoverSrc = isActive ? coverUrl(track.cover_id, 80) : null;

    return (
      <TrackContextMenu
        track={track}
        onPlayNow={() => void handlePlay(index)}
        onRemoveFromPlaylist={
          onRemoveFromPlaylist ? () => onRemoveFromPlaylist(index, track) : undefined
        }
      >
        <div
          onDoubleClick={() => void handlePlay(index)}
          style={
            isActive
              ? {
                  background: [
                    `linear-gradient(96deg, ${hexToRgba(c1, 0.36)} 0%, ${hexToRgba(c2, 0.28)} 34%, ${hexToRgba(c3, 0.24)} 68%, ${hexToRgba(c4, 0.28)} 100%)`,
                    `radial-gradient(ellipse 40% 130% at 14% 50%, ${hexToRgba(c1, 0.28)} 0%, transparent 75%)`,
                    `radial-gradient(ellipse 42% 130% at 56% 50%, ${hexToRgba(c2, 0.24)} 0%, transparent 75%)`,
                    `radial-gradient(ellipse 38% 130% at 88% 50%, ${hexToRgba(c3, 0.22)} 0%, transparent 75%)`,
                  ].join(", "),
                  boxShadow: `inset 0 0 0 1px ${hexToRgba(c1, 0.32)}`,
                }
              : undefined
          }
          className={`group relative grid h-[56px] ${gridColsClass} items-center gap-3 overflow-hidden rounded-xl px-3 transition-[background,box-shadow,color] duration-300 ${
            isActive
              ? ""
              : isSelected
                ? "bg-[#33F3FF]/[0.08] shadow-[inset_0_0_0_1px_rgba(51,243,255,0.28)]"
                : "hover:bg-white/[0.05]"
          }`}
        >
          {isActive && (
            <>
              {activeCoverSrc && (
                <img
                  src={activeCoverSrc}
                  alt=""
                  aria-hidden="true"
                  className="pointer-events-none absolute inset-0 h-full w-full object-cover opacity-35 blur-2xl saturate-[2.4] contrast-125 scale-125"
                />
              )}
              <span
                aria-hidden="true"
                style={{
                  background: `linear-gradient(180deg, ${c1} 0%, ${c2} 36%, ${c3} 70%, ${c4} 100%)`,
                }}
                className="pointer-events-none absolute inset-y-0 left-0 w-[3.5px]"
              />
            </>
          )}

          {selectable ? (
            <button
              type="button"
              role="checkbox"
              aria-checked={isSelected}
              aria-label={`Sélectionner ${track.title}`}
              onClick={(e) => {
                e.stopPropagation();
                onToggleSelect?.(track.id);
              }}
              className={`relative z-10 mx-auto flex h-5 w-5 items-center justify-center rounded-md border transition-colors cursor-pointer ${
                isSelected
                  ? "border-[#33F3FF] bg-[#33F3FF] text-[#040406]"
                  : "border-white/25 bg-white/[0.04] text-transparent hover:border-white/50"
              }`}
            >
              <Check className="h-3.5 w-3.5 stroke-[2.75]" />
            </button>
          ) : (
            <button
              type="button"
              onClick={() => void handlePlay(index)}
              className="relative z-10 flex h-7 w-7 items-center justify-center rounded-md text-xs tabular-nums cursor-pointer mx-auto"
              aria-label={`Lire ${track.title}`}
            >
              {isPlaying ? (
                <AudioEqualizerBars
                  active={isPlaying}
                  accentColor={titleColor}
                  colors={[c1, c2, c3, c4]}
                />
              ) : (
                <>
                  <span
                    style={isActive ? { color: titleColor } : undefined}
                    className={`group-hover:hidden group-focus-within:hidden transition-colors duration-300 ${
                      isActive ? "font-semibold" : "text-white/55"
                    }`}
                  >
                    {showIndex ? index + 1 : "•"}
                  </span>
                  <Play
                    style={isActive ? { color: titleColor } : undefined}
                    className="hidden h-3.5 w-3.5 fill-current text-white group-hover:block group-focus-within:block"
                  />
                </>
              )}
            </button>
          )}

          <div className="relative z-10 flex min-w-0 items-center gap-3">
            {showCover && (
              <CoverImage
                coverId={track.cover_id}
                alt={track.title}
                size={80}
                className="h-10 w-10"
                rounded="md"
              />
            )}
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-1.5">
                <span
                  style={isActive ? { color: titleColor } : undefined}
                  className="truncate text-sm font-medium text-white transition-colors duration-300"
                >
                  {track.title}
                </span>
                {track.explicit && <Badge variant="explicit">E</Badge>}
                {hasDolbyAudio(track) && <DolbyAudioLogo />}
              </div>
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  if (track.artist_id) {
                    navigate({ name: "artist", artistId: track.artist_id });
                  }
                }}
                className="block max-w-full truncate text-xs text-white/60 hover:text-white cursor-pointer"
              >
                {track.artist_name}
              </button>
            </div>
          </div>

          {showAlbum && (
            <div className="relative z-10 hidden md:block min-w-0">
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  if (track.album_id) {
                    navigate({ name: "album", albumId: track.album_id });
                  }
                }}
                className="block max-w-full truncate text-xs text-white/60 hover:text-white cursor-pointer"
              >
                {track.album_title}
              </button>
            </div>
          )}

          <div className="relative z-10 flex justify-end">
            <Badge variant={tier === "MAX" ? "max" : tier === "HIGH" ? "high" : "low"}>
              {tier}
            </Badge>
          </div>

          <div className="relative z-10 flex items-center justify-end gap-2">
            <button
              type="button"
              aria-label={isFav ? "Retirer des favoris" : "Ajouter aux favoris"}
              onClick={async (e) => {
                e.stopPropagation();
                const res = await commands.toggleFavoriteTrack(track);
                if (res.status === "ok") {
                  toggleFavLocal(track, res.data);
                } else {
                  toast.error(res.error);
                }
              }}
              className={`transition-opacity cursor-pointer ${
                isFav
                  ? "text-[#FF2D55] opacity-100"
                  : "text-white/55 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 hover:text-white"
              }`}
            >
              <Heart className="h-3.5 w-3.5" fill={isFav ? "currentColor" : "none"} />
            </button>
            <button
              type="button"
              aria-label="Ajouter à une playlist"
              onClick={(e) => {
                e.stopPropagation();
                openAddToPlaylist(track);
              }}
              className="text-white/55 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 hover:text-white transition-opacity cursor-pointer"
            >
              <MoreHorizontal className="h-3.5 w-3.5" />
            </button>
            <span className="w-10 text-right text-xs text-white/60 tabular-nums">
              {formatSecs(track.duration_secs)}
            </span>
          </div>
        </div>
      </TrackContextMenu>
    );
  };

  return (
    <div className={embedded ? "flex flex-col" : "flex flex-1 min-h-0 flex-col"}>
      <div
        className={`grid ${gridColsClass} items-center gap-3 border-b border-white/[0.06] px-3 py-2 text-[11px] font-semibold uppercase tracking-wider text-white/55`}
      >
        {selectable ? (
          <button
            type="button"
            role="checkbox"
            aria-checked={allSelected}
            aria-label="Tout sélectionner"
            onClick={() => onToggleSelectAll?.()}
            className={`mx-auto flex h-4.5 w-4.5 items-center justify-center rounded border transition-colors cursor-pointer ${
              allSelected
                ? "border-[#33F3FF] bg-[#33F3FF] text-[#040406]"
                : "border-white/25 bg-white/[0.04] text-transparent hover:border-white/50"
            }`}
          >
            <Check className="h-3 w-3 stroke-[2.75]" />
          </button>
        ) : (
          <span className="text-center">#</span>
        )}
        <span>Titre</span>
        {showAlbum && <span className="hidden md:block">Album</span>}
        <span className="text-right">Qualité</span>
        <span className="text-right">Durée</span>
      </div>

      {embedded ? (
        <div className="flex flex-col">
          {tracks.map((track, idx) => (
            <div key={`${track.id}-${idx}`}>{renderTrackRow(track, idx)}</div>
          ))}
        </div>
      ) : (
        <div
          ref={scrollRef}
          className="flex-1 min-h-0 overflow-y-auto"
          style={{ contain: "strict" }}
        >
          <div
            style={{
              height: `${virtualizer.getTotalSize()}px`,
              width: "100%",
              position: "relative",
            }}
          >
            {virtualizer.getVirtualItems().map((vRow) => {
              const track = tracks[vRow.index];
              return (
                <div
                  key={`${track.id}-${vRow.index}`}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    height: `${vRow.size}px`,
                    transform: `translateY(${vRow.start}px)`,
                  }}
                >
                  {renderTrackRow(track, vRow.index)}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
