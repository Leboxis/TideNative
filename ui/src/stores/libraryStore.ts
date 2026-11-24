import { create } from "zustand";
import type {
  AlbumSummary,
  ArtistSummary,
  BootstrapState,
  ExploreHubData,
  LibrarySyncPayload,
  MixSummary,
  PlaylistFolder,
  PlaylistSummary,
  TrackSummary,
} from "@/bindings";

interface LibraryStore {
  favorites: TrackSummary[];
  favoriteIds: Set<number>;
  playlists: PlaylistSummary[];
  playlistFolders: PlaylistFolder[];
  favoriteAlbums: AlbumSummary[];
  favoriteAlbumIds: Set<number>;
  favoriteArtists: ArtistSummary[];
  favoriteArtistIds: Set<number>;
  userMixes: MixSummary[];
  discoverTracks: TrackSummary[];
  recentTracks: TrackSummary[];
  recentSearches: string[];
  exploreHub: ExploreHubData | null;
  hydrate: (boot: BootstrapState) => void;
  applySync: (payload: LibrarySyncPayload) => void;
  setExploreHub: (hub: ExploreHubData) => void;
  toggleFavoriteTrackLocal: (track: TrackSummary, added: boolean) => void;
  toggleFavoriteAlbumLocal: (albumId: number, album: AlbumSummary | null, added: boolean) => void;
  toggleFavoriteArtistLocal: (artistId: number, artist: ArtistSummary | null, added: boolean) => void;
  addPlaylistLocal: (playlist: PlaylistSummary) => void;
  removePlaylistLocal: (uuid: string) => void;
}

export const useLibraryStore = create<LibraryStore>((set) => ({
  favorites: [],
  favoriteIds: new Set(),
  playlists: [],
  playlistFolders: [],
  favoriteAlbums: [],
  favoriteAlbumIds: new Set(),
  favoriteArtists: [],
  favoriteArtistIds: new Set(),
  userMixes: [],
  discoverTracks: [],
  recentTracks: [],
  recentSearches: [],
  exploreHub: null,

  hydrate: (boot) =>
    set({
      favorites: boot.favorites,
      favoriteIds: new Set(boot.favorite_ids),
      playlists: boot.playlists,
      playlistFolders: boot.playlist_folders,
      favoriteAlbums: boot.favorite_albums,
      favoriteAlbumIds: new Set(boot.favorite_album_ids),
      favoriteArtists: boot.favorite_artists,
      favoriteArtistIds: new Set(boot.favorite_artist_ids),
      userMixes: boot.user_mixes,
      discoverTracks: boot.discover_tracks,
      recentTracks: boot.recent_tracks,
      recentSearches: boot.recent_searches,
      exploreHub: boot.explore_hub,
    }),

  applySync: (payload) =>
    set({
      favorites: payload.favorites,
      favoriteIds: new Set(payload.favorite_ids),
      playlists: payload.playlists,
      playlistFolders: payload.playlist_folders,
      favoriteAlbums: payload.favorite_albums,
      favoriteAlbumIds: new Set(payload.favorite_album_ids),
      favoriteArtists: payload.favorite_artists,
      favoriteArtistIds: new Set(payload.favorite_artist_ids),
      userMixes: payload.user_mixes,
      discoverTracks: payload.discover_tracks,
    }),

  setExploreHub: (exploreHub) => set({ exploreHub }),

  toggleFavoriteTrackLocal: (track, added) =>
    set((state) => {
      const nextIds = new Set(state.favoriteIds);
      let nextFavs = state.favorites;
      if (added) {
        nextIds.add(track.id);
        if (!nextFavs.some((t) => t.id === track.id)) {
          nextFavs = [track, ...nextFavs];
        }
      } else {
        nextIds.delete(track.id);
        nextFavs = nextFavs.filter((t) => t.id !== track.id);
      }
      return { favoriteIds: nextIds, favorites: nextFavs };
    }),

  toggleFavoriteAlbumLocal: (albumId, album, added) =>
    set((state) => {
      const nextIds = new Set(state.favoriteAlbumIds);
      let nextAlbums = state.favoriteAlbums;
      if (added) {
        nextIds.add(albumId);
        if (album && !nextAlbums.some((a) => a.id === albumId)) {
          nextAlbums = [album, ...nextAlbums];
        }
      } else {
        nextIds.delete(albumId);
        nextAlbums = nextAlbums.filter((a) => a.id !== albumId);
      }
      return { favoriteAlbumIds: nextIds, favoriteAlbums: nextAlbums };
    }),

  toggleFavoriteArtistLocal: (artistId, artist, added) =>
    set((state) => {
      const nextIds = new Set(state.favoriteArtistIds);
      let nextArtists = state.favoriteArtists;
      if (added) {
        nextIds.add(artistId);
        if (artist && !nextArtists.some((a) => a.id === artistId)) {
          nextArtists = [artist, ...nextArtists];
        }
      } else {
        nextIds.delete(artistId);
        nextArtists = nextArtists.filter((a) => a.id !== artistId);
      }
      return { favoriteArtistIds: nextIds, favoriteArtists: nextArtists };
    }),

  addPlaylistLocal: (playlist) =>
    set((state) => ({ playlists: [playlist, ...state.playlists] })),

  removePlaylistLocal: (uuid) =>
    set((state) => ({ playlists: state.playlists.filter((p) => p.uuid !== uuid) })),
}));
