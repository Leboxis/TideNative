import {
  ArrowDown,
  ArrowUp,
  GripVertical,
  Infinity as InfinityIcon,
  ListPlus,
  Trash2,
  X,
} from "lucide-react";
import { AnimatePresence, Reorder, motion } from "motion/react";
import { type FormEvent, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { type TrackSummary, commands } from "@/bindings";
import { CoverImage } from "@/components/media/CoverImage";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { ActionTooltip } from "@/components/ui/tooltip";
import { formatSecs } from "@/lib/utils";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";

interface QueueRow {
  uid: string;
  track: TrackSummary;
}

function buildQueueRows(tracks: TrackSummary[]): QueueRow[] {
  const counts = new Map<number, number>();
  return tracks.map((track) => {
    const seen = counts.get(track.id) ?? 0;
    counts.set(track.id, seen + 1);
    return {
      uid: `${track.id}#${seen}`,
      track,
    };
  });
}

function defaultQueuePlaylistTitle(): string {
  const now = new Date();
  const dd = String(now.getDate()).padStart(2, "0");
  const mm = String(now.getMonth() + 1).padStart(2, "0");
  return `File d'attente ${dd}/${mm}`;
}

export function QueuePanel() {
  const open = useUiStore((s) => s.queueOpen);
  const setOpen = useUiStore((s) => s.setQueueOpen);
  const queue = usePlaybackStore((s) => s.queue);
  const queueIndex = usePlaybackStore((s) => s.queueIndex);
  const currentTrack = usePlaybackStore((s) => s.currentTrack);
  const preferences = usePlaybackStore((s) => s.preferences);
  const setPreferences = usePlaybackStore((s) => s.setPreferences);
  const addPlaylistLocal = useLibraryStore((s) => s.addPlaylistLocal);

  const upcomingStart = queueIndex !== null ? queueIndex + 1 : 0;
  const upcoming = useMemo(() => queue.slice(upcomingStart), [queue, upcomingStart]);
  const [rows, setRows] = useState<QueueRow[]>(() => buildQueueRows(upcoming));
  const [saveOpen, setSaveOpen] = useState(false);
  const [playlistTitle, setPlaylistTitle] = useState("");
  const [savingPlaylist, setSavingPlaylist] = useState(false);
  const dragStartRef = useRef<{ uid: string; index: number } | null>(null);
  const isDraggingRef = useRef(false);
  const autoplayEnabled = preferences.autoplay_radio ?? true;

  useEffect(() => {
    if (!isDraggingRef.current) {
      setRows(buildQueueRows(upcoming));
    }
  }, [upcoming]);

  const handleToggleAutoplay = async (checked: boolean) => {
    const next = { ...preferences, autoplay_radio: checked };
    setPreferences(next);
    const res = await commands.updatePreferences(next);
    if (res.status === "error") {
      setPreferences(preferences);
      toast.error(res.error);
    }
  };

  const handleMoveOffset = (fromOffset: number, toOffset: number) => {
    if (toOffset < 0 || toOffset >= rows.length || fromOffset === toOffset) return;
    setRows((prev) => {
      const next = [...prev];
      const [moved] = next.splice(fromOffset, 1);
      next.splice(toOffset, 0, moved);
      return next;
    });
    void commands.queueMove(upcomingStart + fromOffset, upcomingStart + toOffset);
  };

  const handleSaveAsPlaylist = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = playlistTitle.trim();
    if (!trimmed || queue.length === 0) return;

    setSavingPlaylist(true);
    const createdRes = await commands.createPlaylist(
      trimmed,
      "Créée depuis la file d'attente TideNative",
    );
    if (createdRes.status === "error") {
      setSavingPlaylist(false);
      toast.error(createdRes.error);
      return;
    }

    let addedCount = 0;
    for (const track of queue) {
      const addRes = await commands.addTrackToPlaylist(createdRes.data.uuid, track.id);
      if (addRes.status === "ok") {
        addedCount += 1;
      }
    }

    addPlaylistLocal({
      ...createdRes.data,
      number_of_tracks: addedCount,
      image_id: createdRes.data.image_id ?? queue[0]?.cover_id ?? null,
    });
    setSavingPlaylist(false);
    setSaveOpen(false);
    setPlaylistTitle("");
    toast.success(`Playlist « ${trimmed} » créée (${addedCount} titres)`);
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.aside
          role="dialog"
          aria-label="File d'attente"
          initial={{ x: "100%", opacity: 0.7 }}
          animate={{ x: 0, opacity: 1 }}
          exit={{ x: "100%", opacity: 0.7 }}
          transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
          className="fixed bottom-[98px] right-4 top-16 z-40 flex w-96 flex-col overflow-hidden rounded-2xl glass-surface"
        >
          <div className="flex items-center justify-between border-b border-white/10 px-4 py-3">
            <div>
              <h2 className="text-sm font-semibold text-white">File d&apos;attente</h2>
              <p className="text-[11px] text-white/60 tabular-nums">
                {rows.length} titre{rows.length > 1 ? "s" : ""} à venir
              </p>
            </div>
            <div className="flex items-center gap-1">
              {queue.length > 0 && (
                <ActionTooltip label="Enregistrer la file en playlist">
                  <Button
                    variant="ghost"
                    size="sm"
                    aria-label="Enregistrer la file en playlist"
                    onClick={() => {
                      setPlaylistTitle((prev) => prev || defaultQueuePlaylistTitle());
                      setSaveOpen((v) => !v);
                    }}
                    className="h-7 px-2 text-xs text-white/75 hover:text-white"
                  >
                    <ListPlus className="h-3.5 w-3.5" />
                    Sauver
                  </Button>
                </ActionTooltip>
              )}
              {rows.length > 0 && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => void commands.queueClear()}
                  className="h-7 px-2 text-xs text-white/70 hover:text-white"
                >
                  Vider
                </Button>
              )}
              <button
                type="button"
                aria-label="Fermer la file"
                onClick={() => setOpen(false)}
                className="flex h-7 w-7 items-center justify-center rounded-lg text-white/65 hover:bg-white/10 hover:text-white cursor-pointer"
              >
                <X className="h-4 w-4" />
              </button>
            </div>
          </div>

          {saveOpen && (
            <form
              onSubmit={(e) => void handleSaveAsPlaylist(e)}
              className="border-b border-white/10 bg-white/[0.03] px-4 py-3 space-y-2"
            >
              <p className="text-[11px] font-medium text-white/75">
                Enregistrer les {queue.length} titres en playlist
              </p>
              <div className="flex items-center gap-2">
                <Input
                  value={playlistTitle}
                  onChange={(e) => setPlaylistTitle(e.target.value)}
                  placeholder="Nom de la playlist"
                  aria-label="Nom de la nouvelle playlist"
                  className="h-8 flex-1 text-xs"
                  autoFocus
                />
                <Button
                  type="submit"
                  size="sm"
                  disabled={savingPlaylist || !playlistTitle.trim()}
                  className="h-8 px-3 text-xs"
                >
                  {savingPlaylist ? "Enregistrement…" : "Créer"}
                </Button>
              </div>
            </form>
          )}

          <div className="flex-1 overflow-y-auto p-3 space-y-4">
            {currentTrack && (
              <div className="space-y-1.5">
                <p className="px-1 text-[10px] font-semibold uppercase tracking-wider text-[#33F3FF]">
                  En cours
                </p>
                <div className="flex items-center gap-3 rounded-xl bg-[#33F3FF]/10 border border-[#33F3FF]/25 p-2">
                  <CoverImage
                    coverId={currentTrack.cover_id}
                    alt={currentTrack.title}
                    size={80}
                    className="h-10 w-10"
                    rounded="md"
                  />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-xs font-semibold text-white">
                      {currentTrack.title}
                    </p>
                    <p className="truncate text-[11px] text-white/65">
                      {currentTrack.artist_name}
                    </p>
                  </div>
                </div>
              </div>
            )}

            <div className="space-y-1.5">
              <div className="flex items-center justify-between px-1">
                <p className="text-[10px] font-semibold uppercase tracking-wider text-white/55">
                  À suivre
                </p>
                {rows.length > 1 && (
                  <span className="text-[10px] text-white/40">Glisser pour réordonner</span>
                )}
              </div>

              {rows.length === 0 ? (
                <p className="px-2 py-8 text-center text-xs text-white/55">
                  {autoplayEnabled
                    ? "Découverte infinie active — des titres similaires s'enchaîneront automatiquement."
                    : "Aucun titre en attente."}
                </p>
              ) : (
                <Reorder.Group
                  axis="y"
                  values={rows}
                  onReorder={setRows}
                  className="space-y-1"
                >
                  {rows.map((row, offset) => {
                    const absIdx = upcomingStart + offset;
                    const { track } = row;
                    return (
                      <Reorder.Item
                        key={row.uid}
                        value={row}
                        onDragStart={() => {
                          isDraggingRef.current = true;
                          dragStartRef.current = { uid: row.uid, index: offset };
                        }}
                        onDragEnd={() => {
                          isDraggingRef.current = false;
                          const start = dragStartRef.current;
                          dragStartRef.current = null;
                          if (!start) return;
                          const endIdx = rows.findIndex((r) => r.uid === start.uid);
                          if (endIdx >= 0 && endIdx !== start.index) {
                            void commands.queueMove(
                              upcomingStart + start.index,
                              upcomingStart + endIdx,
                            );
                          }
                        }}
                        className="group flex items-center gap-2 rounded-xl bg-white/[0.03] px-2 py-1.5 hover:bg-white/[0.07] border border-transparent hover:border-white/10 cursor-grab active:cursor-grabbing"
                      >
                        <GripVertical className="h-3.5 w-3.5 text-white/35 group-hover:text-white/65 shrink-0" />
                        <CoverImage
                          coverId={track.cover_id}
                          alt={track.title}
                          size={80}
                          className="h-9 w-9"
                          rounded="md"
                        />
                        <button
                          type="button"
                          onClick={() => void commands.playQueueIndex(absIdx)}
                          className="min-w-0 flex-1 text-left cursor-pointer"
                        >
                          <p className="truncate text-xs font-medium text-white">{track.title}</p>
                          <p className="truncate text-[11px] text-white/60">{track.artist_name}</p>
                        </button>
                        <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity">
                          <button
                            type="button"
                            aria-label={`Monter ${track.title}`}
                            disabled={offset === 0}
                            onClick={(e) => {
                              e.stopPropagation();
                              handleMoveOffset(offset, offset - 1);
                            }}
                            className="flex h-6 w-6 items-center justify-center rounded text-white/55 hover:bg-white/10 hover:text-white disabled:opacity-25 cursor-pointer"
                          >
                            <ArrowUp className="h-3 w-3" />
                          </button>
                          <button
                            type="button"
                            aria-label={`Descendre ${track.title}`}
                            disabled={offset === rows.length - 1}
                            onClick={(e) => {
                              e.stopPropagation();
                              handleMoveOffset(offset, offset + 1);
                            }}
                            className="flex h-6 w-6 items-center justify-center rounded text-white/55 hover:bg-white/10 hover:text-white disabled:opacity-25 cursor-pointer"
                          >
                            <ArrowDown className="h-3 w-3" />
                          </button>
                        </div>
                        <span className="text-[11px] text-white/55 tabular-nums">
                          {formatSecs(track.duration_secs)}
                        </span>
                        <button
                          type="button"
                          aria-label={`Retirer ${track.title} de la file`}
                          onClick={() => void commands.queueRemove(absIdx)}
                          className="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 text-white/55 hover:text-[#ff453a] transition-opacity cursor-pointer p-1"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </button>
                      </Reorder.Item>
                    );
                  })}
                </Reorder.Group>
              )}
            </div>
          </div>

          <div className="flex items-center justify-between gap-3 border-t border-white/10 px-4 py-3 bg-white/[0.02]">
            <div className="flex items-center gap-2.5 min-w-0">
              <InfinityIcon
                className={`h-4 w-4 shrink-0 ${
                  autoplayEnabled ? "text-[#33F3FF]" : "text-white/45"
                }`}
              />
              <div className="min-w-0">
                <p className="text-xs font-medium text-white">Découverte infinie</p>
                <p className="truncate text-[11px] text-white/60">
                  Enchaîner avec la radio en fin de file
                </p>
              </div>
            </div>
            <Switch
              aria-label="Découverte infinie"
              checked={autoplayEnabled}
              onCheckedChange={(checked) => void handleToggleAutoplay(checked)}
            />
          </div>
        </motion.aside>
      )}
    </AnimatePresence>
  );
}
