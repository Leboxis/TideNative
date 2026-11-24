import {
  Disc3,
  Heart,
  Info,
  ListEnd,
  ListPlus,
  Play,
  Plus,
  Radio,
  Trash2,
  User,
} from "lucide-react";
import type { ReactNode } from "react";
import { toast } from "sonner";
import { type TrackSummary, commands } from "@/bindings";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { useLibraryStore } from "@/stores/libraryStore";
import { useUiStore } from "@/stores/uiStore";

interface TrackContextMenuProps {
  track: TrackSummary;
  children: ReactNode;
  onPlayNow: () => void;
  onRemoveFromPlaylist?: () => void;
}

export function TrackContextMenu({
  track,
  children,
  onPlayNow,
  onRemoveFromPlaylist,
}: TrackContextMenuProps) {
  const isFav = useLibraryStore((s) => s.favoriteIds.has(track.id));
  const toggleFavLocal = useLibraryStore((s) => s.toggleFavoriteTrackLocal);
  const navigate = useUiStore((s) => s.navigate);
  const openCredits = useUiStore((s) => s.openCredits);
  const openAddToPlaylist = useUiStore((s) => s.openAddToPlaylist);

  const handleFavorite = async () => {
    const res = await commands.toggleFavoriteTrack(track);
    if (res.status === "ok") {
      toggleFavLocal(track, res.data);
      toast.success(res.data ? "Ajouté aux favoris" : "Retiré des favoris");
    } else {
      toast.error(res.error);
    }
  };

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem onSelect={onPlayNow}>
          <Play className="h-3.5 w-3.5" />
          Lire maintenant
        </ContextMenuItem>
        <ContextMenuItem
          onSelect={async () => {
            const res = await commands.queuePlayNext(track);
            if (res.status === "ok") toast.success("Ajouté en piste suivante");
          }}
        >
          <ListPlus className="h-3.5 w-3.5" />
          Lire ensuite
        </ContextMenuItem>
        <ContextMenuItem
          onSelect={async () => {
            const res = await commands.queueAdd(track);
            if (res.status === "ok") toast.success("Ajouté à la file d'attente");
          }}
        >
          <ListEnd className="h-3.5 w-3.5" />
          Ajouter à la file
        </ContextMenuItem>

        <ContextMenuSeparator />

        <ContextMenuItem onSelect={() => void handleFavorite()}>
          <Heart
            className={`h-3.5 w-3.5 ${isFav ? "text-[#FF2D55]" : ""}`}
            fill={isFav ? "currentColor" : "none"}
          />
          {isFav ? "Retirer des favoris" : "Ajouter aux favoris"}
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => openAddToPlaylist(track)}>
          <Plus className="h-3.5 w-3.5" />
          Ajouter à une playlist…
        </ContextMenuItem>
        <ContextMenuItem
          onSelect={async () => {
            const res = await commands.playTrackRadio(track.id);
            if (res.status === "error") toast.error(res.error);
          }}
        >
          <Radio className="h-3.5 w-3.5" />
          Lancer la radio du titre
        </ContextMenuItem>

        <ContextMenuSeparator />

        {track.album_id && (
          <ContextMenuItem
            onSelect={() => navigate({ name: "album", albumId: track.album_id! })}
          >
            <Disc3 className="h-3.5 w-3.5" />
            Voir l&apos;album
          </ContextMenuItem>
        )}
        {track.artist_id && (
          <ContextMenuItem
            onSelect={() => navigate({ name: "artist", artistId: track.artist_id! })}
          >
            <User className="h-3.5 w-3.5" />
            Voir l&apos;artiste
          </ContextMenuItem>
        )}
        <ContextMenuItem onSelect={() => openCredits(track)}>
          <Info className="h-3.5 w-3.5" />
          Crédits du titre
        </ContextMenuItem>

        {onRemoveFromPlaylist && (
          <>
            <ContextMenuSeparator />
            <ContextMenuItem
              onSelect={onRemoveFromPlaylist}
              className="text-[#ff453a] focus:text-[#ff453a]"
            >
              <Trash2 className="h-3.5 w-3.5" />
              Retirer de la playlist
            </ContextMenuItem>
          </>
        )}
      </ContextMenuContent>
    </ContextMenu>
  );
}
