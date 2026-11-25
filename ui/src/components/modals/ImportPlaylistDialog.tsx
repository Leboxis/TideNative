import { FileUp } from "lucide-react";
import { useRef, useState } from "react";
import { toast } from "sonner";
import { type PlaylistExportFormat, commands } from "@/bindings";
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

function detectFormatFromFilename(filename: string): PlaylistExportFormat {
  const lower = filename.toLowerCase();
  if (lower.endsWith(".m3u8") || lower.endsWith(".m3u")) {
    return "M3u8";
  }
  return "Csv";
}

function titleFromFilename(filename: string): string {
  return filename.replace(/\.(csv|m3u8?|txt)$/i, "").replace(/[_-]+/g, " ").trim();
}

export function ImportPlaylistDialog() {
  const open = useUiStore((s) => s.importPlaylistOpen);
  const setOpen = useUiStore((s) => s.setImportPlaylistOpen);
  const navigate = useUiStore((s) => s.navigate);
  const addPlaylistLocal = useLibraryStore((s) => s.addPlaylistLocal);

  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [title, setTitle] = useState("");
  const [format, setFormat] = useState<PlaylistExportFormat>("Csv");
  const [content, setContent] = useState("");
  const [fileName, setFileName] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reset = () => {
    setTitle("");
    setFormat("Csv");
    setContent("");
    setFileName(null);
    if (fileInputRef.current) fileInputRef.current.value = "";
  };

  const handleFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const text = await file.text();
    setFileName(file.name);
    setContent(text);
    setFormat(detectFormatFromFilename(file.name));
    if (!title.trim()) {
      setTitle(titleFromFilename(file.name));
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim() || !content.trim()) return;
    setBusy(true);
    const res = await commands.importPlaylist(title.trim(), content, format);
    setBusy(false);
    if (res.status === "ok") {
      const { playlist, imported_count, unmatched } = res.data;
      addPlaylistLocal(playlist);
      if (unmatched.length > 0) {
        toast.success(
          `Playlist « ${playlist.title} » importée (${imported_count} titres, ${unmatched.length} introuvable${unmatched.length > 1 ? "s" : ""})`,
        );
      } else {
        toast.success(
          `Playlist « ${playlist.title} » importée (${imported_count} titre${imported_count > 1 ? "s" : ""})`,
        );
      }
      reset();
      setOpen(false);
      navigate({ name: "playlist", playlistUuid: playlist.uuid });
    } else {
      toast.error(res.error);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) reset();
        setOpen(next);
      }}
    >
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>Importer une playlist</DialogTitle>
          <DialogDescription>
            Importez un fichier CSV ou M3U8 pour recréer une playlist sur votre compte TIDAL.
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1.5">
            <label htmlFor="import-pl-title" className="text-xs font-medium text-white/70">
              Nom de la playlist
            </label>
            <Input
              id="import-pl-title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Ex. Mes favoris"
              autoFocus
            />
          </div>

          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex items-center gap-1.5">
              <button
                type="button"
                onClick={() => setFormat("Csv")}
                className={`rounded-full px-3 py-1 text-xs font-medium border transition-colors cursor-pointer ${
                  format === "Csv"
                    ? "bg-white text-[#050507] border-white"
                    : "bg-white/[0.04] text-white/70 border-white/10 hover:border-white/20"
                }`}
              >
                CSV
              </button>
              <button
                type="button"
                onClick={() => setFormat("M3u8")}
                className={`rounded-full px-3 py-1 text-xs font-medium border transition-colors cursor-pointer ${
                  format === "M3u8"
                    ? "bg-white text-[#050507] border-white"
                    : "bg-white/[0.04] text-white/70 border-white/10 hover:border-white/20"
                }`}
              >
                M3U8
              </button>
            </div>

            <div>
              <input
                ref={fileInputRef}
                type="file"
                accept=".csv,.m3u,.m3u8,text/csv,audio/x-mpegurl"
                onChange={(e) => void handleFileChange(e)}
                className="hidden"
                aria-label="Choisir un fichier playlist"
              />
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={() => fileInputRef.current?.click()}
              >
                <FileUp className="h-3.5 w-3.5" />
                {fileName ? fileName : "Choisir un fichier…"}
              </Button>
            </div>
          </div>

          <div className="space-y-1.5">
            <label htmlFor="import-pl-content" className="text-xs font-medium text-white/70">
              Contenu {format === "Csv" ? "CSV" : "M3U8"}
            </label>
            <textarea
              id="import-pl-content"
              rows={6}
              value={content}
              onChange={(e) => setContent(e.target.value)}
              placeholder={
                format === "Csv"
                  ? "title,artist,album,track_id\nDigital Love,Daft Punk,Discovery,"
                  : "#EXTM3U\n#EXTINF:298,Daft Punk - Digital Love\nhttps://listen.tidal.com/track/555"
              }
              className="w-full rounded-xl border border-white/10 bg-white/[0.03] px-3 py-2 text-xs text-white placeholder:text-white/30 focus:border-[#33F3FF]/50 focus:outline-none font-mono"
            />
          </div>

          <div className="flex justify-end gap-2 pt-2">
            <Button type="button" variant="ghost" onClick={() => setOpen(false)}>
              Annuler
            </Button>
            <Button type="submit" disabled={!title.trim() || !content.trim() || busy}>
              {busy ? "Importation…" : "Importer"}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
