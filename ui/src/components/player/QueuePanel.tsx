import { GripVertical, Trash2, X } from "lucide-react";
import { AnimatePresence, Reorder, motion } from "motion/react";
import { type TrackSummary, commands } from "@/bindings";
import { CoverImage } from "@/components/media/CoverImage";
import { Button } from "@/components/ui/button";
import { formatSecs } from "@/lib/utils";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";

export function QueuePanel() {
  const open = useUiStore((s) => s.queueOpen);
  const setOpen = useUiStore((s) => s.setQueueOpen);
  const queue = usePlaybackStore((s) => s.queue);
  const queueIndex = usePlaybackStore((s) => s.queueIndex);
  const currentTrack = usePlaybackStore((s) => s.currentTrack);

  const upcomingStart = queueIndex !== null ? queueIndex + 1 : 0;
  const upcoming = queue.slice(upcomingStart);

  const handleReorder = (nextUpcoming: TrackSummary[]) => {
    // Identify moved element and dispatch single move command to Rust engine.
    for (let i = 0; i < nextUpcoming.length; i++) {
      if (upcoming[i]?.id !== nextUpcoming[i]?.id) {
        const fromOffset = upcoming.findIndex((t) => t.id === nextUpcoming[i].id);
        if (fromOffset >= 0 && fromOffset !== i) {
          void commands.queueMove(upcomingStart + fromOffset, upcomingStart + i);
        }
        break;
      }
    }
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.aside
          initial={{ x: "100%", opacity: 0.7 }}
          animate={{ x: 0, opacity: 1 }}
          exit={{ x: "100%", opacity: 0.7 }}
          transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
          className="fixed bottom-[98px] right-4 top-16 z-40 flex w-96 flex-col overflow-hidden rounded-2xl glass-surface"
        >
          <div className="flex items-center justify-between border-b border-white/10 px-4 py-3">
            <div>
              <h2 className="text-sm font-semibold text-white">File d&apos;attente</h2>
              <p className="text-[11px] text-white/45 tabular-nums">
                {upcoming.length} titre{upcoming.length > 1 ? "s" : ""} à venir
              </p>
            </div>
            <div className="flex items-center gap-1">
              {upcoming.length > 0 && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => void commands.queueClear()}
                  className="text-xs text-white/60 hover:text-white"
                >
                  Vider
                </Button>
              )}
              <button
                type="button"
                aria-label="Fermer la file"
                onClick={() => setOpen(false)}
                className="flex h-7 w-7 items-center justify-center rounded-lg text-white/50 hover:bg-white/10 hover:text-white cursor-pointer"
              >
                <X className="h-4 w-4" />
              </button>
            </div>
          </div>

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
                    <p className="truncate text-[11px] text-white/55">{currentTrack.artist_name}</p>
                  </div>
                </div>
              </div>
            )}

            <div className="space-y-1.5">
              <p className="px-1 text-[10px] font-semibold uppercase tracking-wider text-white/40">
                À suivre (glisser pour réordonner)
              </p>
              {upcoming.length === 0 ? (
                <p className="py-10 text-center text-xs text-white/35">
                  Aucun titre en attente.
                </p>
              ) : (
                <Reorder.Group
                  axis="y"
                  values={upcoming}
                  onReorder={handleReorder}
                  className="space-y-1"
                >
                  {upcoming.map((track, offset) => {
                    const absIdx = upcomingStart + offset;
                    return (
                      <Reorder.Item
                        key={`${track.id}-${absIdx}`}
                        value={track}
                        className="group flex items-center gap-2 rounded-xl bg-white/[0.03] px-2 py-1.5 hover:bg-white/[0.07] border border-transparent hover:border-white/10 cursor-grab active:cursor-grabbing"
                      >
                        <GripVertical className="h-3.5 w-3.5 text-white/25 group-hover:text-white/55 shrink-0" />
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
                          <p className="truncate text-[11px] text-white/45">{track.artist_name}</p>
                        </button>
                        <span className="text-[11px] text-white/35 tabular-nums">
                          {formatSecs(track.duration_secs)}
                        </span>
                        <button
                          type="button"
                          aria-label="Retirer de la file"
                          onClick={() => void commands.queueRemove(absIdx)}
                          className="opacity-0 group-hover:opacity-100 text-white/40 hover:text-[#ff453a] transition-opacity cursor-pointer p-1"
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
        </motion.aside>
      )}
    </AnimatePresence>
  );
}
