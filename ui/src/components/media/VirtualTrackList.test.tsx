import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { TrackSummary } from "@/bindings";
import { VirtualTrackList } from "./VirtualTrackList";

function makeTracks(count: number): TrackSummary[] {
  return Array.from({ length: count }, (_, i) => ({
    id: i + 1,
    title: `Track #${i + 1}`,
    artist_name: "Audiophile Artist",
    artist_id: 1,
    album_id: 1,
    album_title: "Master Reference",
    cover_id: null,
    duration_secs: 200,
    audio_quality: "HI_RES_LOSSLESS",
    explicit: false,
  }));
}

describe("VirtualTrackList", () => {
  it("renders only the visible virtualized subset for a 1000-track list", () => {
    const origHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetHeight");
    const origWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth");
    Object.defineProperty(HTMLElement.prototype, "offsetHeight", { configurable: true, value: 600 });
    Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, value: 1000 });
    try {
      render(<VirtualTrackList tracks={makeTracks(1000)} />);
      const playButtons = screen.getAllByRole("button", { name: /^Lire Track #/i });
      expect(playButtons.length).toBeGreaterThan(0);
      expect(playButtons.length).toBeLessThan(40);
    } finally {
      if (origHeight) Object.defineProperty(HTMLElement.prototype, "offsetHeight", origHeight);
      if (origWidth) Object.defineProperty(HTMLElement.prototype, "offsetWidth", origWidth);
    }
  });
});
