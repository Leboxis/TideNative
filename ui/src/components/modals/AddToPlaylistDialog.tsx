import { ListMusic, Plus } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useLibraryStore } from "@/stores/libraryStore";
import { useUiStore } from "@/stores/uiStore";

export function AddToPlaylistDialog() {
  const singleTrack = useUiStore((s) => s.addToPlaylistTrack);
  const batchTracks = useUiStore((s) => s.addToPlaylistTracks);
  const openAddToPlaylist = useUiStore((s) => s.openAddToPlaylist);
  const setCreatePlaylistOpen = useUiStore((s) => s.setCreatePlaylistOpen);
  const playlists = useLibraryStore((s) => s.playlists);
  const [submitting, setSubmitting] = useState(false);

  const targets = batchTracks.length > 0 ? batchTracks : singleTrack ? [singleTrack] : [];
  const open = targets.length > 0;

  const handleClose = () => {
    if (submitting) return;
    openAddToPlaylist(null);
  };

  const handleAdd = async (playlistUuid: string, playlistTitle: string) => {
    if (targets.length === 0 || submitting) return;
    setSubmitting(true);

    if (targets.length === 1) {
      const res = await commands.addTrackToPlaylist(playlistUuid, targets[0].id);
      setSubmitting(false);
      if (res.status === "ok") {
        toast.success(`Ajouté à « ${playlistTitle} »`);
        openAddToPlaylist(null);
      } else {
        toast.error(res.error);
      }
      return;
    }

    let added = 0;
    let lastError: string | null = null;
    for (const track of targets) {
      const res = await commands.addTrackToPlaylist(playlistUuid, track.id);
      if (res.status === "ok") {
        added += 1;
      } else {
        lastError = res.error;
      }
    }
    setSubmitting(false);

    if (added > 0) {
      toast.success(`${added} titre${added > 1 ? "s" : ""} ajouté${added > 1 ? "s" : ""} à « ${playlistTitle} »`);
      openAddToPlaylist(null);
    } else if (lastError) {
      toast.error(lastError);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && handleClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Ajouter à une playlist</DialogTitle>
          {targets.length === 1 ? (
            <DialogDescription className="truncate">
              {targets[0].title} — {targets[0].artist_name}
            </DialogDescription>
          ) : targets.length > 1 ? (
            <DialogDescription>
              {targets.length} titres sélectionnés
            </DialogDescription>
          ) : null}
        </DialogHeader>

        <div className="max-h-72 space-y-1 overflow-y-auto pr-1">
          {playlists.length === 0 ? (
            <p className="py-6 text-center text-xs text-white/45">Aucune playlist trouvée.</p>
          ) : (
            playlists.map((pl) => (
              <button
                key={pl.uuid}
                type="button"
                disabled={submitting}
                onClick={() => void handleAdd(pl.uuid, pl.title)}
                className="flex w-full items-center gap-3 rounded-xl px-3 py-2 text-left text-sm text-white/85 hover:bg-white/10 hover:text-white disabled:opacity-50 transition-colors cursor-pointer"
              >
                <ListMusic className="h-4 w-4 text-white/45 shrink-0" />
                <span className="truncate flex-1 font-medium">{pl.title}</span>
                <span className="text-xs text-white/40 tabular-nums">{pl.number_of_tracks}</span>
              </button>
            ))
          )}
        </div>

        <div className="mt-4 flex justify-between border-t border-white/10 pt-3">
          <Button
            variant="secondary"
            size="sm"
            disabled={submitting}
            onClick={() => {
              openAddToPlaylist(null);
              setCreatePlaylistOpen(true);
            }}
          >
            <Plus className="h-3.5 w-3.5" />
            Nouvelle playlist
          </Button>
          <Button variant="ghost" size="sm" disabled={submitting} onClick={handleClose}>
            Fermer
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
