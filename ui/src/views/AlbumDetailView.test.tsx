import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { type AlbumDetailData, commands } from "@/bindings";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";
import { AlbumDetailView } from "./AlbumDetailView";

const SAMPLE_ALBUM_DATA: AlbumDetailData = {
  album: {
    id: 101,
    title: "Discovery",
    artist_name: "Daft Punk",
    artist_id: 27,
    cover_id: null,
    release_date: "2001-03-12",
    audio_quality: "LOSSLESS",
    number_of_tracks: 2,
  },
  tracks: [
    {
      id: 501,
      title: "One More Time",
      artist_name: "Daft Punk",
      artist_id: 27,
      album_title: "Discovery",
      album_id: 101,
      duration_secs: 320,
      audio_quality: "LOSSLESS",
      cover_id: null,
      explicit: false,
    },
  ],
  is_favorite: false,
  similar_albums: [
    {
      id: 202,
      title: "Cross",
      artist_name: "Justice",
      artist_id: 88,
      cover_id: null,
      release_date: "2007-06-11",
      audio_quality: "LOSSLESS",
      number_of_tracks: 12,
    },
  ],
};

describe("AlbumDetailView", () => {
  beforeEach(() => {
    useLibraryStore.setState({ favoriteAlbumIds: new Set() });
    useUiStore.setState({
      route: { name: "album", albumId: 101 },
      history: [{ name: "album", albumId: 101 }],
      historyIndex: 0,
    });
  });

  it("displays similar albums and navigates on click", async () => {
    const getAlbumSpy = vi
      .spyOn(commands, "getAlbum")
      .mockResolvedValue({ status: "ok", data: SAMPLE_ALBUM_DATA });

    render(<AlbumDetailView albumId={101} />);

    expect(await screen.findByText("Albums similaires")).toBeInTheDocument();
    const similarBtn = screen.getByRole("button", { name: /^cross/i });
    fireEvent.click(similarBtn);

    expect(useUiStore.getState().route).toEqual({ name: "album", albumId: 202 });
    getAlbumSpy.mockRestore();
  });

  it("quick-plays a similar album card without navigating away", async () => {
    const getAlbumSpy = vi
      .spyOn(commands, "getAlbum")
      .mockResolvedValue({ status: "ok", data: SAMPLE_ALBUM_DATA });
    const playSpy = vi
      .spyOn(commands, "playTracks")
      .mockResolvedValue({ status: "ok", data: null });

    render(<AlbumDetailView albumId={101} />);

    const quickPlayBtn = await screen.findByRole("button", { name: "Lire l'album Cross" });
    fireEvent.click(quickPlayBtn);

    await waitFor(() => expect(getAlbumSpy).toHaveBeenCalledWith(202));
    await waitFor(() => expect(playSpy).toHaveBeenCalledWith(SAMPLE_ALBUM_DATA.tracks, 0));
    expect(useUiStore.getState().route).toEqual({ name: "album", albumId: 101 });

    getAlbumSpy.mockRestore();
    playSpy.mockRestore();
  });

  it("launches album radio when clicking Radio album", async () => {
    const getAlbumSpy = vi
      .spyOn(commands, "getAlbum")
      .mockResolvedValue({ status: "ok", data: SAMPLE_ALBUM_DATA });
    const radioSpy = vi
      .spyOn(commands, "playAlbumRadio")
      .mockResolvedValue({ status: "ok", data: null });

    render(<AlbumDetailView albumId={101} />);

    const radioBtn = await screen.findByRole("button", { name: /radio album/i });
    fireEvent.click(radioBtn);

    await waitFor(() => expect(radioSpy).toHaveBeenCalledWith(101));
    getAlbumSpy.mockRestore();
    radioSpy.mockRestore();
  });

  it("disables leftover shuffle when clicking Lire sequentially and uses 4-col grid without album column", async () => {
    usePlaybackStore.setState((s) => ({
      preferences: { ...s.preferences, shuffle_enabled: true },
    }));
    const getAlbumSpy = vi
      .spyOn(commands, "getAlbum")
      .mockResolvedValue({ status: "ok", data: SAMPLE_ALBUM_DATA });
    const shuffleSpy = vi
      .spyOn(commands, "setShuffle")
      .mockResolvedValue({ status: "ok", data: null });
    const playSpy = vi
      .spyOn(commands, "playTracks")
      .mockResolvedValue({ status: "ok", data: null });

    render(<AlbumDetailView albumId={101} />);

    const playBtn = await screen.findByRole("button", { name: /^lire$/i });
    const headerRow = screen.getByText("Qualité").parentElement;
    expect(headerRow?.className).toContain("grid-cols-[40px_1fr_84px_104px]");
    expect(headerRow?.className).not.toContain("200px");
    fireEvent.click(playBtn);

    await waitFor(() => expect(shuffleSpy).toHaveBeenCalledWith(false));
    await waitFor(() => expect(playSpy).toHaveBeenCalledWith(SAMPLE_ALBUM_DATA.tracks, 0));

    getAlbumSpy.mockRestore();
    shuffleSpy.mockRestore();
    playSpy.mockRestore();
  });

  it("disables play buttons when album has no playable tracks", async () => {
    const getAlbumSpy = vi.spyOn(commands, "getAlbum").mockResolvedValue({
      status: "ok",
      data: { ...SAMPLE_ALBUM_DATA, tracks: [] },
    });

    render(<AlbumDetailView albumId={101} />);

    const playBtn = await screen.findByRole("button", { name: /^lire$/i });
    const shuffleBtn = screen.getByRole("button", { name: /aléatoire/i });
    expect(playBtn).toBeDisabled();
    expect(shuffleBtn).toBeDisabled();

    getAlbumSpy.mockRestore();
  });
});
