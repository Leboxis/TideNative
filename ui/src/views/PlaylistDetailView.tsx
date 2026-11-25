import {
  ArrowDown,
  ArrowUp,
  Check,
  Copy,
  Download,
  GripVertical,
  ListOrdered,
  Pencil,
  Play,
  Shuffle,
  Trash2,
} from "lucide-react";
import { Reorder } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import {
  type PlaylistDetailData,
  type PlaylistExportFormat,
  type TrackSummary,
  commands,
} from "@/bindings";
import { CoverImage } from "@/components/media/CoverImage";
import { DetailHeaderSurface } from "@/components/media/DetailHeaderSurface";
import { VirtualTrackList } from "@/components/media/VirtualTrackList";
import { Button } from "@/components/ui/button";
import { DetailHeaderSkeleton, TrackListSkeleton } from "@/components/ui/skeleton";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { formatSecs } from "@/lib/utils";
import { useLibraryStore } from "@/stores/libraryStore";
import { useUiStore } from "@/stores/uiStore";

interface PlaylistDetailViewProps {
  playlistUuid: string;
}

interface ReorderRow {
  uid: string;
  track: TrackSummary;
}

function triggerFileDownload(filename: string, content: string, mimeType: string) {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function moveArrayItem<T>(list: T[], from: number, to: number): T[] {
  const next = [...list];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}

export function PlaylistDetailView({ playlistUuid }: PlaylistDetailViewProps) {
  const [data, setData] = useState<PlaylistDetailData | null>(null);
  const [loading, setLoading] = useState(true);
  const [editOpen, setEditOpen] = useState(false);
  const [editTitle, setEditTitle] = useState("");
  const [editDesc, setEditDesc] = useState("");
  const [savingEdit, setSavingEdit] = useState(false);
  const [duplicating, setDuplicating] = useState(false);
  const [exporting, setExporting] = useState(false);

  const [reorderMode, setReorderMode] = useState(false);
  const [reorderRows, setReorderRows] = useState<ReorderRow[]>([]);
  const [movingTrack, setMovingTrack] = useState(false);
  const dragStartRef = useRef<{ uid: string; index: number } | null>(null);

  const addPlaylistLocal = useLibraryStore((s) => s.addPlaylistLocal);
  const updatePlaylistLocal = useLibraryStore((s) => s.updatePlaylistLocal);
  const removePlaylistLocal = useLibraryStore((s) => s.removePlaylistLocal);
  const navigate = useUiStore((s) => s.navigate);

  const load = useCallback(async () => {
    setLoading(true);
    const res = await commands.getPlaylistTracks(playlistUuid);
    setLoading(false);
    if (res.status === "ok") {
      setData(res.data);
      setReorderRows(
        res.data.tracks.map((track, idx) => ({
          uid: `${track.id}-${idx}`,
          track,
        })),
      );
    } else {
      toast.error(res.error);
    }
  }, [playlistUuid]);

  useEffect(() => {
    setReorderMode(false);
    void load();
  }, [load]);

  if (loading) {
    return (
      <div className="space-y-6 pb-4">
        <DetailHeaderSkeleton />
        <TrackListSkeleton rows={7} />
      </div>
    );
  }

  if (!data) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 text-sm text-white/60">
        <p>Playlist introuvable.</p>
        <Button variant="secondary" size="sm" onClick={() => navigate({ name: "collection" })}>
          Retour à ma collection
        </Button>
      </div>
    );
  }

  const openEditDialog = () => {
    setEditTitle(data.playlist.title);
    setEditDesc(data.playlist.description ?? "");
    setEditOpen(true);
  };

  const handleSaveEdit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!editTitle.trim()) return;
    setSavingEdit(true);
    const res = await commands.updatePlaylist(playlistUuid, editTitle.trim(), editDesc.trim());
    setSavingEdit(false);
    if (res.status === "ok") {
      const updated = {
        ...data.playlist,
        title: res.data.title,
        description: res.data.description,
      };
      setData({ ...data, playlist: updated });
      updatePlaylistLocal(updated);
      toast.success("Playlist mise à jour");
      setEditOpen(false);
    } else {
      toast.error(res.error);
    }
  };

  const handleDuplicate = async () => {
    setDuplicating(true);
    const res = await commands.duplicatePlaylist(playlistUuid, null);
    setDuplicating(false);
    if (res.status === "ok") {
      addPlaylistLocal(res.data);
      toast.success(`Playlist dupliquée : « ${res.data.title} »`);
      navigate({ name: "playlist", playlistUuid: res.data.uuid });
    } else {
      toast.error(res.error);
    }
  };

  const handleExport = async (format: PlaylistExportFormat) => {
    setExporting(true);
    const res = await commands.exportPlaylist(playlistUuid, format);
    setExporting(false);
    if (res.status === "ok") {
      triggerFileDownload(res.data.filename, res.data.content, res.data.mime_type);
      toast.success(`Playlist exportée (${res.data.filename})`);
    } else {
      toast.error(res.error);
    }
  };

  const commitMove = async (fromIdx: number, toIdx: number, optimisticRows?: ReorderRow[]) => {
    if (fromIdx === toIdx || fromIdx < 0 || toIdx < 0 || toIdx >= data.tracks.length) {
      return;
    }
    const prevData = data;
    const prevRows = reorderRows;
    const nextRows = optimisticRows ?? moveArrayItem(reorderRows, fromIdx, toIdx);
    const nextTracks = nextRows.map((r) => r.track);
    const fromTidal = data.tidal_indices[fromIdx] ?? fromIdx;
    const toTidal = data.tidal_indices[toIdx] ?? toIdx;

    setReorderRows(nextRows);
    setData({ ...data, tracks: nextTracks });
    setMovingTrack(true);
    const res = await commands.movePlaylistTrack(playlistUuid, fromTidal, toTidal);
    setMovingTrack(false);

    if (res.status === "error") {
      setData(prevData);
      setReorderRows(prevRows);
      toast.error(res.error);
    }
  };

  const handleRemoveTrack = async (displayIdx: number, track: TrackSummary) => {
    const tidalIndex = data.tidal_indices[displayIdx] ?? displayIdx;
    const res = await commands.removeTrackFromPlaylist(playlistUuid, tidalIndex, track.id);
    if (res.status === "ok") {
      toast.success("Titre retiré de la playlist");
      void load();
    } else {
      toast.error(res.error);
    }
  };

  const handleDeletePlaylist = async () => {
    const res = await commands.deletePlaylist(playlistUuid);
    if (res.status === "ok") {
      removePlaylistLocal(playlistUuid);
      toast.success("Playlist supprimée");
      navigate({ name: "collection", initialTab: "playlists" });
    } else {
      toast.error(res.error);
    }
  };

  return (
    <div className="flex flex-1 min-h-0 flex-col space-y-6 pb-4">
      <DetailHeaderSurface
        coverId={data.playlist.image_id ?? data.tracks[0]?.cover_id}
        className="flex flex-col gap-6 md:flex-row md:items-end"
      >
        <CoverImage
          coverId={data.playlist.image_id ?? data.tracks[0]?.cover_id}
          alt={data.playlist.title}
          size={640}
          className="h-44 w-44 shadow-2xl"
          rounded="2xl"
        />
        <div className="min-w-0 flex-1 space-y-2">
          <p className="text-xs font-semibold uppercase tracking-widest text-white/45">Playlist</p>
          <h1 className="text-3xl font-bold tracking-tight text-white">{data.playlist.title}</h1>
          {data.playlist.description && (
            <p className="text-xs text-white/55 max-w-2xl">{data.playlist.description}</p>
          )}
          <p className="text-xs text-white/45 tabular-nums">
            {data.playlist.creator ? `${data.playlist.creator} • ` : ""}
            {data.tracks.length} titres
          </p>

          <div className="flex flex-wrap items-center gap-2.5 pt-2">
            <Button
              disabled={data.tracks.length === 0}
              onClick={() => void commands.playTracks(data.tracks, 0)}
            >
              <Play className="h-4 w-4 fill-current" />
              Lire
            </Button>
            <Button
              variant="secondary"
              disabled={data.tracks.length === 0}
              onClick={async () => {
                await commands.setShuffle(true);
                await commands.playTracks(data.tracks, 0);
              }}
            >
              <Shuffle className="h-4 w-4" />
              Aléatoire
            </Button>
            <Button variant="outline" size="sm" onClick={openEditDialog}>
              <Pencil className="h-3.5 w-3.5" />
              Modifier
            </Button>
            {data.tracks.length > 1 && (
              <Button
                variant={reorderMode ? "default" : "outline"}
                size="sm"
                onClick={() => {
                  setReorderRows(
                    data.tracks.map((track, idx) => ({
                      uid: `${track.id}-${idx}`,
                      track,
                    })),
                  );
                  setReorderMode((v) => !v);
                }}
              >
                {reorderMode ? (
                  <>
                    <Check className="h-3.5 w-3.5" />
                    Terminer
                  </>
                ) : (
                  <>
                    <ListOrdered className="h-3.5 w-3.5" />
                    Réorganiser
                  </>
                )}
              </Button>
            )}
            <Button
              variant="outline"
              size="sm"
              disabled={duplicating}
              onClick={() => void handleDuplicate()}
            >
              <Copy className="h-3.5 w-3.5" />
              {duplicating ? "Duplication…" : "Dupliquer"}
            </Button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="outline" size="sm" disabled={exporting || data.tracks.length === 0}>
                  <Download className="h-3.5 w-3.5" />
                  Exporter
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start">
                <DropdownMenuItem onSelect={() => void handleExport("Csv")}>
                  Exporter en CSV (.csv)
                </DropdownMenuItem>
                <DropdownMenuItem onSelect={() => void handleExport("M3u8")}>
                  Exporter en M3U8 (.m3u8)
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
            <Button
              variant="destructive"
              size="sm"
              onClick={() => void handleDeletePlaylist()}
            >
              <Trash2 className="h-3.5 w-3.5" />
              Supprimer
            </Button>
          </div>
        </div>
      </DetailHeaderSurface>

      {reorderMode ? (
        <div className="flex flex-1 min-h-0 flex-col space-y-2">
          <p className="text-xs text-white/50">
            Glissez un titre ou utilisez les flèches pour modifier l&apos;ordre de la playlist.
          </p>
          <div className="flex-1 overflow-y-auto pr-1">
            <Reorder.Group
              axis="y"
              values={reorderRows}
              onReorder={setReorderRows}
              className="space-y-1.5"
            >
              {reorderRows.map((row, idx) => (
                <Reorder.Item
                  key={row.uid}
                  value={row}
                  onDragStart={() => {
                    dragStartRef.current = { uid: row.uid, index: idx };
                  }}
                  onDragEnd={() => {
                    const start = dragStartRef.current;
                    dragStartRef.current = null;
                    if (!start) return;
                    const endIdx = reorderRows.findIndex((r) => r.uid === start.uid);
                    if (endIdx >= 0 && endIdx !== start.index) {
                      void commitMove(start.index, endIdx, reorderRows);
                    }
                  }}
                  className="flex items-center gap-3 rounded-xl border border-white/[0.08] bg-white/[0.03] px-3 py-2 hover:border-white/15 cursor-grab active:cursor-grabbing"
                >
                  <GripVertical className="h-4 w-4 text-white/30 shrink-0" />
                  <span className="w-6 text-center text-xs text-white/40 tabular-nums">
                    {idx + 1}
                  </span>
                  <CoverImage
                    coverId={row.track.cover_id}
                    alt={row.track.title}
                    size={80}
                    className="h-9 w-9 shrink-0"
                    rounded="md"
                  />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium text-white">{row.track.title}</p>
                    <p className="truncate text-xs text-white/50">{row.track.artist_name}</p>
                  </div>
                  <span className="text-xs text-white/40 tabular-nums">
                    {formatSecs(row.track.duration_secs)}
                  </span>
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      disabled={movingTrack || idx === 0}
                      aria-label={`Monter ${row.track.title}`}
                      onClick={() => void commitMove(idx, idx - 1)}
                      className="flex h-7 w-7 items-center justify-center rounded-lg text-white/55 hover:bg-white/10 hover:text-white disabled:opacity-25 cursor-pointer"
                    >
                      <ArrowUp className="h-3.5 w-3.5" />
                    </button>
                    <button
                      type="button"
                      disabled={movingTrack || idx === reorderRows.length - 1}
                      aria-label={`Descendre ${row.track.title}`}
                      onClick={() => void commitMove(idx, idx + 1)}
                      className="flex h-7 w-7 items-center justify-center rounded-lg text-white/55 hover:bg-white/10 hover:text-white disabled:opacity-25 cursor-pointer"
                    >
                      <ArrowDown className="h-3.5 w-3.5" />
                    </button>
                  </div>
                </Reorder.Item>
              ))}
            </Reorder.Group>
          </div>
        </div>
      ) : (
        <VirtualTrackList tracks={data.tracks} onRemoveFromPlaylist={handleRemoveTrack} />
      )}

      <Dialog open={editOpen} onOpenChange={setEditOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Modifier la playlist</DialogTitle>
            <DialogDescription>
              Mettez à jour le nom et la description de votre playlist TIDAL.
            </DialogDescription>
          </DialogHeader>
          <form onSubmit={handleSaveEdit} className="space-y-4">
            <div className="space-y-1.5">
              <label htmlFor="edit-pl-title" className="text-xs font-medium text-white/70">
                Nom
              </label>
              <Input
                id="edit-pl-title"
                value={editTitle}
                onChange={(e) => setEditTitle(e.target.value)}
                autoFocus
              />
            </div>
            <div className="space-y-1.5">
              <label htmlFor="edit-pl-desc" className="text-xs font-medium text-white/70">
                Description
              </label>
              <Input
                id="edit-pl-desc"
                value={editDesc}
                onChange={(e) => setEditDesc(e.target.value)}
                placeholder="Description optionnelle…"
              />
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button type="button" variant="ghost" onClick={() => setEditOpen(false)}>
                Annuler
              </Button>
              <Button type="submit" disabled={!editTitle.trim() || savingEdit}>
                {savingEdit ? "Enregistrement…" : "Enregistrer"}
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
