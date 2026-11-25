import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "@/bindings";
import { telemetryRuntime } from "@/lib/telemetry";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";
import { NowPlayingView, activeLyricsIndex } from "./NowPlayingView";

describe("activeLyricsIndex", () => {
  const lines = [
    { time_ms: 5_000, text: "First" },
    { time_ms: 12_000, text: "Second" },
    { time_ms: 25_000, text: "Third" },
  ];

  it("binary searches the active synced lyric line for any playback timestamp", () => {
    expect(activeLyricsIndex(lines, 1_000)).toBe(-1);
    expect(activeLyricsIndex(lines, 5_000)).toBe(0);
    expect(activeLyricsIndex(lines, 14_500)).toBe(1);
    expect(activeLyricsIndex(lines, 99_000)).toBe(2);
  });
});

describe("NowPlayingView controls & Up Next tab", () => {
  if (typeof globalThis.ResizeObserver === "undefined") {
    globalThis.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    };
  }

  const track1 = {
    id: 1,
    title: "Instant Crush",
    artist_name: "Daft Punk",
    artist_id: 27,
    album_id: 101,
    album_title: "Random Access Memories",
    cover_id: null,
    duration_secs: 337,
    audio_quality: "HI_RES_LOSSLESS",
    explicit: false,
  };
  const track2 = {
    id: 2,
    title: "Touch",
    artist_name: "Daft Punk",
    artist_id: 27,
    album_id: 101,
    album_title: "Random Access Memories",
    cover_id: null,
    duration_secs: 498,
    audio_quality: "HI_RES_LOSSLESS",
    explicit: false,
  };

  beforeEach(() => {
    useUiStore.setState({
      nowPlayingOpen: true,
      route: { name: "discover" },
      history: [{ name: "discover" }],
      historyIndex: 0,
    });
    useLibraryStore.setState({ favoriteIds: new Set() });
    usePlaybackStore.setState({
      playbackState: "Playing",
      currentTrack: track1,
      queue: [track1, track2],
      queueIndex: 0,
      durationMs: 337_000,
    });
  });

  it("dispatches shuffle, repeat, favorite, artist navigation, and Up Next queue playback", async () => {
    const lyricsSpy = vi.spyOn(commands, "getLyrics").mockResolvedValue({
      status: "ok",
      data: { track_id: 1, synced_lines: [], plain_text: null, provider: null },
    });
    const creditsSpy = vi.spyOn(commands, "getCredits").mockResolvedValue({
      status: "ok",
      data: { track_id: 1, groups: [] },
    });
    const shuffleSpy = vi.spyOn(commands, "setShuffle").mockResolvedValue({ status: "ok", data: null });
    const repeatSpy = vi.spyOn(commands, "cycleRepeat").mockResolvedValue({ status: "ok", data: "All" });
    const favSpy = vi.spyOn(commands, "toggleFavoriteTrack").mockResolvedValue({ status: "ok", data: true });
    const playIdxSpy = vi.spyOn(commands, "playQueueIndex").mockResolvedValue({ status: "ok", data: null });

    render(<NowPlayingView />);

    fireEvent.click(screen.getByRole("button", { name: "Aléatoire" }));
    expect(shuffleSpy).toHaveBeenCalledWith(true);

    fireEvent.click(screen.getByRole("button", { name: "Répétition" }));
    expect(repeatSpy).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "Ajouter aux favoris" }));
    expect(favSpy).toHaveBeenCalledWith(track1);

    const queueTab = screen.getByRole("tab", { name: /à suivre/i });
    fireEvent.mouseDown(queueTab);
    fireEvent.click(queueTab);
    fireEvent.click(await screen.findByText("Touch"));
    expect(playIdxSpy).toHaveBeenCalledWith(1);

    fireEvent.click(screen.getByRole("button", { name: "Daft Punk" }));
    expect(useUiStore.getState().nowPlayingOpen).toBe(false);
    expect(useUiStore.getState().route).toEqual({ name: "artist", artistId: 27 });

    lyricsSpy.mockRestore();
    creditsSpy.mockRestore();
    shuffleSpy.mockRestore();
    repeatSpy.mockRestore();
    favSpy.mockRestore();
    playIdxSpy.mockRestore();
  });

  it("pauses synced lyrics auto-scroll on manual wheel and resumes on button click", async () => {
    telemetryRuntime.seedPosition(13_000, 337_000);
    const scrollIntoViewMock = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoViewMock;

    const lyricsSpy = vi.spyOn(commands, "getLyrics").mockResolvedValue({
      status: "ok",
      data: {
        track_id: 1,
        synced_lines: [
          { time_ms: 5_000, text: "First line" },
          { time_ms: 12_000, text: "Second line" },
          { time_ms: 25_000, text: "Third line" },
        ],
        plain_text: null,
        provider: "TIDAL",
      },
    });
    const creditsSpy = vi.spyOn(commands, "getCredits").mockResolvedValue({
      status: "ok",
      data: { track_id: 1, groups: [] },
    });

    render(<NowPlayingView />);

    await screen.findByText("Second line");
    const container = screen.getByTestId("lyrics-scroll-container");
    expect(container.className).toContain("lyrics-fade-mask");

    expect(screen.queryByRole("button", { name: /revenir à la ligne en cours/i })).toBeNull();

    fireEvent.wheel(container, { deltaY: 120 });
    const resumeBtn = await screen.findByRole("button", {
      name: /revenir à la ligne en cours/i,
    });
    expect(resumeBtn).toBeInTheDocument();

    scrollIntoViewMock.mockClear();
    fireEvent.click(resumeBtn);

    expect(screen.queryByRole("button", { name: /revenir à la ligne en cours/i })).toBeNull();
    expect(scrollIntoViewMock).toHaveBeenCalledTimes(1);

    lyricsSpy.mockRestore();
    creditsSpy.mockRestore();
  });
});
