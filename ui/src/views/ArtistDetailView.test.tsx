import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "@/bindings";
import { useLibraryStore } from "@/stores/libraryStore";
import { useUiStore } from "@/stores/uiStore";
import { ArtistDetailView } from "./ArtistDetailView";

describe("ArtistDetailView", () => {
  beforeEach(() => {
    useLibraryStore.setState({ favoriteArtistIds: new Set() });
    useUiStore.setState({
      route: { name: "artist", artistId: 27 },
      history: [{ name: "artist", artistId: 27 }],
      historyIndex: 0,
    });
  });

  it("renders expandable bio, EPs & Singles, Compilations tabs, and navigates to similar artists", async () => {
    const longBio = "Daft Punk est un duo électronique français formé à Paris en 1993. ".repeat(6);
    const spy = vi.spyOn(commands, "getArtist").mockResolvedValue({
      status: "ok",
      data: {
        artist: { id: 27, name: "Daft Punk", picture_id: null },
        bio: longBio,
        top_tracks: [],
        albums: [
          {
            id: 101,
            title: "Discovery",
            artist_name: "Daft Punk",
            artist_id: 27,
            cover_id: null,
            release_date: "2001-03-12",
            audio_quality: "LOSSLESS",
            number_of_tracks: 14,
          },
        ],
        eps_singles: [
          {
            id: 202,
            title: "One More Time",
            artist_name: "Daft Punk",
            artist_id: 27,
            cover_id: null,
            release_date: "2000-11-30",
            audio_quality: "LOSSLESS",
            number_of_tracks: 2,
          },
        ],
        compilations: [
          {
            id: 303,
            title: "Musique Vol. 1",
            artist_name: "Daft Punk",
            artist_id: 27,
            cover_id: null,
            release_date: "2006-04-04",
            audio_quality: "LOSSLESS",
            number_of_tracks: 14,
          },
        ],
        similar_artists: [{ id: 88, name: "Justice", picture_id: null }],
      },
    });

    render(<ArtistDetailView artistId={27} />);

    expect(await screen.findByText("Discovery")).toBeInTheDocument();

    const expandBtn = screen.getByRole("button", { name: /voir plus/i });
    fireEvent.click(expandBtn);
    expect(screen.getByRole("button", { name: /voir moins/i })).toBeInTheDocument();

    const epsTab = screen.getByRole("tab", { name: /eps & singles/i });
    fireEvent.mouseDown(epsTab);
    fireEvent.click(epsTab);
    expect(await screen.findByText("One More Time")).toBeInTheDocument();

    const compTab = screen.getByRole("tab", { name: /compilations/i });
    fireEvent.mouseDown(compTab);
    fireEvent.click(compTab);
    expect(await screen.findByText("Musique Vol. 1")).toBeInTheDocument();

    expect(screen.getByText("Artistes similaires")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /justice/i }));
    expect(useUiStore.getState().route).toEqual({ name: "artist", artistId: 88 });

    spy.mockRestore();
  });
});
