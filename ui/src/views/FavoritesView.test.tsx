import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "@/bindings";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";
import { FavoritesView } from "./FavoritesView";

const FAVORITE_TRACKS = [
  {
    id: 101,
    title: "Giorgio by Moroder",
    artist_name: "Daft Punk",
    artist_id: 27,
    album_id: 1,
    album_title: "Random Access Memories",
    cover_id: null,
    duration_secs: 544,
    audio_quality: "HI_RES_LOSSLESS",
    explicit: false,
  },
  {
    id: 102,
    title: "Digital Love",
    artist_name: "Daft Punk",
    artist_id: 27,
    album_id: 2,
    album_title: "Discovery",
    cover_id: null,
    duration_secs: 301,
    audio_quality: "LOSSLESS",
    explicit: false,
  },
  {
    id: 103,
    title: "Spatial Mix Demo",
    artist_name: "Dolby Lab",
    artist_id: 99,
    album_id: 3,
    album_title: "Atmos Sessions",
    cover_id: null,
    duration_secs: 180,
    audio_quality: "HI_RES_LOSSLESS",
    explicit: false,
    dolby_audio: true,
  },
];

describe("FavoritesView quality filters, multi-select & Dolby Audio badge", () => {
  beforeEach(() => {
    usePlaybackStore.setState({ authenticated: true });
    useLibraryStore.setState({
      favorites: FAVORITE_TRACKS,
      favoriteIds: new Set(FAVORITE_TRACKS.map((t) => t.id)),
    });
    useUiStore.setState({
      addToPlaylistTrack: null,
      addToPlaylistTracks: [],
    });
  });

  it("filters favorites by quality tier and renders the square Dolby Audio badge only on Dolby tracks", () => {
    render(<FavoritesView />);

    expect(screen.getByText("Giorgio by Moroder")).toBeInTheDocument();
    expect(screen.getByText("Digital Love")).toBeInTheDocument();

    const dolbyBadges = screen.getAllByLabelText("Dolby Audio");
    expect(dolbyBadges).toHaveLength(1);
    expect(dolbyBadges[0].parentElement?.textContent).toContain("Spatial Mix Demo");

    fireEvent.click(screen.getByRole("button", { name: /^MAX/i }));
    expect(screen.getByText("Giorgio by Moroder")).toBeInTheDocument();
    expect(screen.queryByText("Digital Love")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /^Toutes qualités/i }));
    expect(screen.getByText("Digital Love")).toBeInTheDocument();
  });

  it("selects multiple favorites and queues or sends them to AddToPlaylistDialog", async () => {
    const queueAddSpy = vi
      .spyOn(commands, "queueAdd")
      .mockResolvedValue({ status: "ok", data: null });

    render(<FavoritesView />);

    fireEvent.click(screen.getByRole("button", { name: /sélectionner/i }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Sélectionner Giorgio by Moroder" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Sélectionner Digital Love" }));

    expect(screen.getByText("2 sélectionnés")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /^Playlist$/i }));
    expect(useUiStore.getState().addToPlaylistTracks).toHaveLength(2);

    fireEvent.click(screen.getByRole("button", { name: /à la file/i }));
    await waitFor(() => expect(queueAddSpy).toHaveBeenCalledTimes(2));

    queueAddSpy.mockRestore();
  });
});
