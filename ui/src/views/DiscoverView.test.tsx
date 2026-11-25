import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "@/bindings";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";
import { DiscoverView } from "./DiscoverView";

const FAVORITE_A = {
  id: 1,
  title: "Old Favorite One",
  artist_name: "Artist One",
  artist_id: 10,
  album_id: 100,
  album_title: "Fav Album",
  cover_id: null,
  duration_secs: 210,
  audio_quality: "LOSSLESS",
  explicit: false,
  dolby_audio: false,
};

const DISCOVER_A = {
  id: 501,
  title: "Fresh Discovery Alpha",
  artist_name: "New Wave Artist",
  artist_id: 50,
  album_id: 500,
  album_title: "Horizon",
  cover_id: null,
  duration_secs: 245,
  audio_quality: "HI_RES_LOSSLESS",
  explicit: false,
  dolby_audio: true,
};

const DISCOVER_B = {
  id: 502,
  title: "Fresh Discovery Beta",
  artist_name: "Second Artist",
  artist_id: 51,
  album_id: 501,
  album_title: "Pulse",
  cover_id: null,
  duration_secs: 198,
  audio_quality: "LOSSLESS",
  explicit: false,
  dolby_audio: false,
};

describe("DiscoverView startup stability & personalized feed", () => {
  beforeEach(() => {
    usePlaybackStore.setState({ authenticated: true });
    useUiStore.setState({
      route: { name: "discover" },
      history: [{ name: "discover" }],
      historyIndex: 0,
    });
    useLibraryStore.setState({
      favorites: [FAVORITE_A],
      favoriteIds: new Set([FAVORITE_A.id]),
      favoriteAlbums: [],
      userMixes: [],
      discoverTracks: [],
      recentTracks: [],
      initialSyncPending: false,
    });
  });

  it("renders a loading skeleton instead of flashing favorites during initial cold sync", () => {
    useLibraryStore.setState({
      favorites: [FAVORITE_A],
      discoverTracks: [],
      userMixes: [],
      initialSyncPending: true,
    });

    render(<DiscoverView />);

    expect(screen.getByTestId("discover-loading-skeleton")).toBeInTheDocument();
    expect(screen.queryByText("Old Favorite One")).not.toBeInTheDocument();
  });

  it("shows the skeleton while a startup renewal is pending, even with persisted tracks", () => {
    useLibraryStore.setState({
      favorites: [FAVORITE_A],
      discoverTracks: [DISCOVER_A, DISCOVER_B],
      userMixes: [],
      initialSyncPending: true,
    });

    render(<DiscoverView />);

    expect(screen.getByTestId("discover-loading-skeleton")).toBeInTheDocument();
    expect(screen.queryByText("Fresh Discovery Alpha")).not.toBeInTheDocument();
  });

  it("displays persisted discover tracks immediately, switches mood tabs, and triggers refreshDiscover", async () => {
    useLibraryStore.setState({
      favorites: [FAVORITE_A],
      favoriteAlbums: [
        {
          id: 901,
          title: "Discovery",
          artist_name: "Daft Punk",
          artist_id: 27,
          cover_id: null,
          release_date: "2001-03-12",
          number_of_tracks: 14,
          audio_quality: "HI_RES_LOSSLESS",
        },
      ],
      playlists: [
        {
          uuid: "pl-1",
          title: "Late Night Hi-Res",
          description: null,
          creator: "Moi",
          number_of_tracks: 18,
          image_id: null,
        },
      ],
      discoverTracks: [DISCOVER_A, DISCOVER_B],
      initialSyncPending: false,
    });
    const refreshSpy = vi
      .spyOn(commands, "refreshDiscover")
      .mockResolvedValue({ status: "ok", data: [DISCOVER_B, DISCOVER_A] });

    render(<DiscoverView />);

    expect(screen.getByTestId("discover-split-layout").className).toContain("7fr");
    expect(screen.getByTestId("discover-split-layout").className).toContain("3fr");
    expect(screen.getByTestId("discover-primary-column").className).toContain("lg:overflow-y-auto");
    expect(screen.getByTestId("discover-secondary-column").className).toContain("lg:overflow-y-auto");
    expect(screen.getByText("Albums sauvegardés")).toBeInTheDocument();
    expect(screen.getByText("Discovery")).toBeInTheDocument();
    expect(screen.getByText("Vos playlists")).toBeInTheDocument();
    expect(screen.getByText("Late Night Hi-Res")).toBeInTheDocument();

    expect(screen.getAllByText("Fresh Discovery Alpha").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("Fresh Discovery Beta")).toBeInTheDocument();
    expect(screen.getAllByLabelText("Dolby Audio").length).toBeGreaterThanOrEqual(1);

    fireEvent.click(screen.getByRole("tab", { name: "À redécouvrir" }));
    expect(screen.getAllByText("Old Favorite One").length).toBeGreaterThanOrEqual(1);

    fireEvent.click(screen.getByRole("button", { name: /varier la sélection/i }));
    await waitFor(() => expect(refreshSpy).toHaveBeenCalledTimes(1));

    refreshSpy.mockRestore();
  });
});
