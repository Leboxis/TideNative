import { ListMusic, Plus } from "lucide-react";
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
  const track = useUiStore((s) => s.addToPlaylistTrack);
  const openAddToPlaylist = useUiStore((s) => s.openAddToPlaylist);
  const setCreatePlaylistOpen = useUiStore((s) => s.setCreatePlaylistOpen);
  const playlists = useLibraryStore((s) => s.playlists);

  const handleAdd = async (playlistUuid: string, playlistTitle: string) => {
    if (!track) return;
    const res = await commands.addTrackToPlaylist(playlistUuid, track.id);
    if (res.status === "ok") {
      toast.success(`Ajouté à « ${playlistTitle} »`);
      openAddToPlaylist(null);
    } else {
      toast.error(res.error);
    }
  };

  return (
    <Dialog open={track !== null} onOpenChange={(o) => !o && openAddToPlaylist(null)}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Ajouter à une playlist</DialogTitle>
          {track && (
            <DialogDescription className="truncate">
              {track.title} — {track.artist_name}
            </DialogDescription>
          )}
        </DialogHeader>

        <div className="max-h-72 space-y-1 overflow-y-auto pr-1">
          {playlists.length === 0 ? (
            <p className="py-6 text-center text-xs text-white/45">Aucune playlist trouvée.</p>
          ) : (
            playlists.map((pl) => (
              <button
                key={pl.uuid}
                type="button"
                onClick={() => void handleAdd(pl.uuid, pl.title)}
                className="flex w-full items-center gap-3 rounded-xl px-3 py-2 text-left text-sm text-white/85 hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
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
            onClick={() => {
              openAddToPlaylist(null);
              setCreatePlaylistOpen(true);
            }}
          >
            <Plus className="h-3.5 w-3.5" />
            Nouvelle playlist
          </Button>
          <Button variant="ghost" size="sm" onClick={() => openAddToPlaylist(null)}>
            Fermer
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
