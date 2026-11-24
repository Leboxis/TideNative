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
import { Input } from "@/components/ui/input";
import { useLibraryStore } from "@/stores/libraryStore";
import { useUiStore } from "@/stores/uiStore";

export function CreatePlaylistDialog() {
  const open = useUiStore((s) => s.createPlaylistOpen);
  const setOpen = useUiStore((s) => s.setCreatePlaylistOpen);
  const addPlaylistLocal = useLibraryStore((s) => s.addPlaylistLocal);

  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [busy, setBusy] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim()) return;
    setBusy(true);
    const res = await commands.createPlaylist(title.trim(), description.trim());
    setBusy(false);
    if (res.status === "ok") {
      addPlaylistLocal(res.data);
      toast.success(`Playlist « ${res.data.title} » créée`);
      setTitle("");
      setDescription("");
      setOpen(false);
    } else {
      toast.error(res.error);
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Nouvelle playlist</DialogTitle>
          <DialogDescription>
            Créez une playlist synchronisée directement sur votre compte TIDAL.
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1.5">
            <label htmlFor="pl-title" className="text-xs font-medium text-white/70">
              Nom
            </label>
            <Input
              id="pl-title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Ex. Late Night Hi-Res"
              autoFocus
            />
          </div>
          <div className="space-y-1.5">
            <label htmlFor="pl-desc" className="text-xs font-medium text-white/70">
              Description (optionnel)
            </label>
            <Input
              id="pl-desc"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Notes ou ambiance…"
            />
          </div>
          <div className="flex justify-end gap-2 pt-2">
            <Button type="button" variant="ghost" onClick={() => setOpen(false)}>
              Annuler
            </Button>
            <Button type="submit" disabled={!title.trim() || busy}>
              {busy ? "Création…" : "Créer"}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
