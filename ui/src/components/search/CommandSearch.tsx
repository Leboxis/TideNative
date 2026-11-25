import { Clock, Disc3, ListMusic, Music2, Search, User } from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { type HybridSearchResults, type SearchSuggestions, type TopHit, commands } from "@/bindings";
import { CoverImage } from "@/components/media/CoverImage";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { useLibraryStore } from "@/stores/libraryStore";
import { useUiStore } from "@/stores/uiStore";

export function CommandSearch() {
  const open = useUiStore((s) => s.commandOpen);
  const setOpen = useUiStore((s) => s.setCommandOpen);
  const navigate = useUiStore((s) => s.navigate);
  const recentSearches = useLibraryStore((s) => s.recentSearches);

  const [query, setQuery] = useState("");
  const [suggestions, setSuggestions] = useState<SearchSuggestions | null>(null);
  const [results, setResults] = useState<HybridSearchResults | null>(null);
  const seqRef = useRef(0);

  useEffect(() => {
    if (!open) {
      setQuery("");
      setSuggestions(null);
      setResults(null);
      return;
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const trimmed = query.trim();
    if (!trimmed) {
      setSuggestions(null);
      setResults(null);
      return;
    }
    const mySeq = ++seqRef.current;
    const timer = window.setTimeout(async () => {
      const [sugRes, hybRes] = await Promise.all([
        commands.searchSuggestions(trimmed),
        commands.searchHybrid(trimmed),
      ]);
      if (seqRef.current !== mySeq) return;
      if (sugRes.status === "ok") setSuggestions(sugRes.data);
      if (hybRes.status === "ok") setResults(hybRes.data);
    }, 180);
    return () => window.clearTimeout(timer);
  }, [query, open]);

  const closeAnd = (fn: () => void) => {
    setOpen(false);
    fn();
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="max-w-xl p-0 overflow-hidden border-[#33F3FF]/25 shadow-[0_24px_80px_rgba(0,0,0,0.85),0_0_28px_rgba(51,243,255,0.12)]">
        <DialogTitle className="sr-only">Recherche rapide TIDAL</DialogTitle>
        <motion.div
          initial={{ opacity: 0, scale: 0.94, y: -14 }}
          animate={{ opacity: 1, scale: 1, y: 0 }}
          transition={{ type: "spring", stiffness: 460, damping: 30, mass: 0.65 }}
        >
          <Command shouldFilter={false}>
          <CommandInput
            value={query}
            onValueChange={setQuery}
            placeholder="Rechercher un titre, album, artiste ou playlist…"
          />
          <CommandList>
            {!query.trim() && recentSearches.length > 0 && (
              <CommandGroup heading="Recherches récentes">
                {recentSearches.map((item) => (
                  <CommandItem key={item} onSelect={() => setQuery(item)}>
                    <Clock className="h-3.5 w-3.5 text-white/40" />
                    <span>{item}</span>
                  </CommandItem>
                ))}
              </CommandGroup>
            )}

            {query.trim() && (
              <>
                <CommandItem
                  onSelect={() =>
                    closeAnd(() => navigate({ name: "explore", initialQuery: query.trim() }))
                  }
                >
                  <Search className="h-3.5 w-3.5 text-[#33F3FF]" />
                  <span>
                    Explorer tous les résultats pour «{" "}
                    <strong className="text-white">{query.trim()}</strong> »
                  </span>
                </CommandItem>

                {results?.remote.top_hit && (
                  <CommandGroup heading="Meilleur résultat">
                    <TopHitCommandItem
                      topHit={results.remote.top_hit}
                      onSelect={(action) => closeAnd(action)}
                      onNavigate={navigate}
                    />
                  </CommandGroup>
                )}

                {results && results.local_tracks.length > 0 && (
                  <CommandGroup heading="Votre bibliothèque (FTS5)">
                    {results.local_tracks.slice(0, 4).map((track, idx) => (
                      <CommandItem
                        key={`loc-${track.id}`}
                        onSelect={() =>
                          closeAnd(() => void commands.playTracks(results.local_tracks, idx))
                        }
                      >
                        <CoverImage coverId={track.cover_id} alt={track.title} size={80} className="h-8 w-8" rounded="md" />
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-sm font-medium text-white">{track.title}</p>
                          <p className="truncate text-xs text-white/45">{track.artist_name}</p>
                        </div>
                      </CommandItem>
                    ))}
                  </CommandGroup>
                )}

                {results && results.remote.tracks.length > 0 && (
                  <CommandGroup heading="Titres TIDAL">
                    {results.remote.tracks.slice(0, 6).map((track, idx) => (
                      <CommandItem
                        key={`rem-${track.id}`}
                        onSelect={() =>
                          closeAnd(() => void commands.playTracks(results.remote.tracks, idx))
                        }
                      >
                        <CoverImage coverId={track.cover_id} alt={track.title} size={80} className="h-8 w-8" rounded="md" />
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-sm font-medium text-white">{track.title}</p>
                          <p className="truncate text-xs text-white/45">
                            {track.artist_name} • {track.album_title}
                          </p>
                        </div>
                        <Music2 className="h-3.5 w-3.5 text-white/30" />
                      </CommandItem>
                    ))}
                  </CommandGroup>
                )}

                {results && results.remote.albums.length > 0 && (
                  <CommandGroup heading="Albums">
                    {results.remote.albums.slice(0, 4).map((album) => (
                      <CommandItem
                        key={`alb-${album.id}`}
                        onSelect={() =>
                          closeAnd(() => navigate({ name: "album", albumId: album.id }))
                        }
                      >
                        <CoverImage coverId={album.cover_id} alt={album.title} size={80} className="h-8 w-8" rounded="md" />
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-sm font-medium text-white">{album.title}</p>
                          <p className="truncate text-xs text-white/45">{album.artist_name}</p>
                        </div>
                        <Disc3 className="h-3.5 w-3.5 text-white/30" />
                      </CommandItem>
                    ))}
                  </CommandGroup>
                )}

                {(results?.remote.artists.length || suggestions?.artists.length) ? (
                  <CommandGroup heading="Artistes">
                    {(results?.remote.artists.length
                      ? results.remote.artists
                      : suggestions?.artists ?? []
                    )
                      .slice(0, 4)
                      .map((artist) => (
                        <CommandItem
                          key={`art-${artist.id}`}
                          onSelect={() =>
                            closeAnd(() => navigate({ name: "artist", artistId: artist.id }))
                          }
                        >
                          <CoverImage coverId={artist.picture_id} alt={artist.name} size={160} className="h-8 w-8" rounded="full" />
                          <span className="truncate text-sm font-medium text-white">{artist.name}</span>
                          <User className="ml-auto h-3.5 w-3.5 text-white/30" />
                        </CommandItem>
                      ))}
                  </CommandGroup>
                ) : null}

                <CommandEmpty>Aucun résultat trouvé.</CommandEmpty>
              </>
            )}
          </CommandList>
          </Command>
        </motion.div>
      </DialogContent>
    </Dialog>
  );
}

function TopHitCommandItem({
  topHit,
  onSelect,
  onNavigate,
}: {
  topHit: TopHit;
  onSelect: (fn: () => void) => void;
  onNavigate: ReturnType<typeof useUiStore.getState>["navigate"];
}) {
  if (topHit.kind === "Artist") {
    const artist = topHit.value;
    return (
      <CommandItem
        key={`top-art-${artist.id}`}
        onSelect={() => onSelect(() => onNavigate({ name: "artist", artistId: artist.id }))}
      >
        <CoverImage
          coverId={artist.picture_id}
          alt={artist.name}
          size={160}
          className="h-9 w-9"
          rounded="full"
        />
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-semibold text-white">{artist.name}</p>
          <p className="truncate text-xs text-[#33F3FF]">Artiste</p>
        </div>
        <User className="h-3.5 w-3.5 text-white/30" />
      </CommandItem>
    );
  }
  if (topHit.kind === "Album") {
    const album = topHit.value;
    return (
      <CommandItem
        key={`top-alb-${album.id}`}
        onSelect={() => onSelect(() => onNavigate({ name: "album", albumId: album.id }))}
      >
        <CoverImage
          coverId={album.cover_id}
          alt={album.title}
          size={80}
          className="h-9 w-9"
          rounded="md"
        />
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-semibold text-white">{album.title}</p>
          <p className="truncate text-xs text-white/50">Album • {album.artist_name}</p>
        </div>
        <Disc3 className="h-3.5 w-3.5 text-[#33F3FF]/70" />
      </CommandItem>
    );
  }
  if (topHit.kind === "Playlist") {
    const pl = topHit.value;
    return (
      <CommandItem
        key={`top-pl-${pl.uuid}`}
        onSelect={() => onSelect(() => onNavigate({ name: "playlist", playlistUuid: pl.uuid }))}
      >
        <CoverImage
          coverId={pl.image_id}
          alt={pl.title}
          size={80}
          className="h-9 w-9"
          rounded="md"
        />
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-semibold text-white">{pl.title}</p>
          <p className="truncate text-xs text-white/50">Playlist • {pl.number_of_tracks} titres</p>
        </div>
        <ListMusic className="h-3.5 w-3.5 text-[#33F3FF]/70" />
      </CommandItem>
    );
  }
  const track = topHit.value;
  return (
    <CommandItem
      key={`top-trk-${track.id}`}
      onSelect={() => onSelect(() => void commands.playTracks([track], 0))}
    >
      <CoverImage
        coverId={track.cover_id}
        alt={track.title}
        size={80}
        className="h-9 w-9"
        rounded="md"
      />
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-semibold text-white">{track.title}</p>
        <p className="truncate text-xs text-white/50">
          Titre • {track.artist_name}
        </p>
      </div>
      <Music2 className="h-3.5 w-3.5 text-[#33F3FF]/70" />
    </CommandItem>
  );
}
