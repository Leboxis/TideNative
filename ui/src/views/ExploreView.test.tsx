import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "@/bindings";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";
import { ExploreView } from "./ExploreView";

describe("ExploreView topHits", () => {
  beforeEach(() => {
    usePlaybackStore.setState({ authenticated: true });
    useLibraryStore.setState({
      exploreHub: {
        category_covers: {},
        spotlight_albums: [],
        new_release_tracks: [],
        featured_playlists: [],
        discovery_tracks: [],
      },
    });
    useUiStore.setState({
      route: { name: "explore", initialQuery: "Daft Punk" },
      history: [{ name: "explore", initialQuery: "Daft Punk" }],
      historyIndex: 0,
    });
  });

  it("renders the top_hit banner and navigates to the matched artist", async () => {
    const searchSpy = vi.spyOn(commands, "searchHybrid").mockResolvedValue({
      status: "ok",
      data: {
        seq: 1,
        query: "Daft Punk",
        local_tracks: [],
        remote: {
          tracks: [],
          albums: [],
          playlists: [],
          artists: [{ id: 27, name: "Daft Punk", picture_id: null }],
          top_hit: {
            kind: "Artist",
            value: { id: 27, name: "Daft Punk", picture_id: null },
          },
        },
      },
    });

    render(<ExploreView initialQuery="Daft Punk" />);

    expect(await screen.findByText("Meilleur résultat")).toBeInTheDocument();
    const openBtn = screen.getByRole("button", { name: /ouvrir daft punk/i });
    fireEvent.click(openBtn);

    expect(useUiStore.getState().route).toEqual({ name: "artist", artistId: 27 });
    searchSpy.mockRestore();
  });
});
