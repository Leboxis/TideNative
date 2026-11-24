import { describe, expect, it } from "vitest";
import { activeLyricsIndex } from "./NowPlayingView";

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
