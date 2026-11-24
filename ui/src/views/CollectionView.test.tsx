import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { TopNavPill } from "@/components/layout/TopNavPill";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";
import { CollectionView } from "./CollectionView";

describe("CollectionView & navigation", () => {
  beforeEach(() => {
    usePlaybackStore.setState({ authenticated: true });
    useUiStore.setState({
      route: { name: "collection", initialTab: "albums" },
      history: [{ name: "collection", initialTab: "albums" }],
      historyIndex: 0,
      createPlaylistOpen: false,
    });
    useLibraryStore.setState({
      favorites: [],
      favoriteIds: new Set(),
      favoriteAlbums: [
        {
          id: 101,
          title: "Random Access Memories",
          artist_name: "Daft Punk",
          artist_id: 10,
          cover_id: null,
          release_date: "2013-05-17",
          audio_quality: "HI_RES_LOSSLESS",
          number_of_tracks: 13,
        },
      ],
      favoriteAlbumIds: new Set([101]),
      favoriteArtists: [{ id: 10, name: "Daft Punk", picture_id: null }],
      favoriteArtistIds: new Set([10]),
      playlists: [
        {
          uuid: "pl-1",
          title: "Late Night Hi-Res",
          description: null,
          creator: "User",
          number_of_tracks: 24,
          image_id: null,
        },
        {
          uuid: "pl-2",
          title: "Studio Reference",
          description: null,
          creator: "User",
          number_of_tracks: 12,
          image_id: null,
        },
      ],
      playlistFolders: [
        {
          id: "folder-jazz",
          name: "Hi-Res Jazz",
          total_number_of_items: 1,
          playlists: [
            {
              uuid: "pl-1",
              title: "Late Night Hi-Res",
              description: null,
              creator: "User",
              number_of_tracks: 24,
              image_id: null,
            },
          ],
        },
      ],
    });
  });

  it("navigates to collection from TopNavPill", () => {
    useUiStore.setState({
      route: { name: "discover" },
      history: [{ name: "discover" }],
      historyIndex: 0,
    });
    render(<TopNavPill />);
    fireEvent.click(screen.getByRole("button", { name: /collection/i }));
    expect(useUiStore.getState().route).toEqual({ name: "collection" });
  });

  it("displays saved albums and navigates to album detail on click", () => {
    render(<CollectionView initialTab="albums" />);
    const albumBtn = screen.getByRole("button", { name: /random access memories/i });
    fireEvent.click(albumBtn);
    expect(useUiStore.getState().route).toEqual({ name: "album", albumId: 101 });
  });

  it("displays followed artists and navigates to artist detail on click", () => {
    render(<CollectionView initialTab="artists" />);
    const artistBtn = screen.getByRole("button", { name: /daft punk/i });
    fireEvent.click(artistBtn);
    expect(useUiStore.getState().route).toEqual({ name: "artist", artistId: 10 });
  });

  it("filters playlists by folder and opens CreatePlaylistDialog", () => {
    render(<CollectionView initialTab="playlists" />);
    expect(screen.getByText("Late Night Hi-Res")).toBeInTheDocument();
    expect(screen.getByText("Studio Reference")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /hi-res jazz/i }));
    expect(screen.getByText("Late Night Hi-Res")).toBeInTheDocument();
    expect(screen.queryByText("Studio Reference")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /nouvelle playlist/i }));
    expect(useUiStore.getState().createPlaylistOpen).toBe(true);
  });
});
