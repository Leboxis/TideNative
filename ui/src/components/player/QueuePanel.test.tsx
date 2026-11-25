import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "@/bindings";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";
import { QueuePanel } from "./QueuePanel";

const TRACK_A = {
  id: 10,
  title: "Track Alpha",
  artist_name: "Artist A",
  artist_id: 1,
  album_id: 100,
  album_title: "Album A",
  cover_id: null,
  duration_secs: 200,
  audio_quality: "HI_RES_LOSSLESS",
  explicit: false,
};

const TRACK_B = {
  id: 11,
  title: "Track Beta",
  artist_name: "Artist B",
  artist_id: 2,
  album_id: 101,
  album_title: "Album B",
  cover_id: null,
  duration_secs: 210,
  audio_quality: "LOSSLESS",
  explicit: false,
};

const TRACK_C = {
  id: 12,
  title: "Track Gamma",
  artist_name: "Artist C",
  artist_id: 3,
  album_id: 102,
  album_title: "Album C",
  cover_id: null,
  duration_secs: 185,
  audio_quality: "LOSSLESS",
  explicit: false,
};

describe("QueuePanel", () => {
  beforeEach(() => {
    useUiStore.setState({ queueOpen: true });
    useLibraryStore.setState({ playlists: [] });
    usePlaybackStore.setState({
      currentTrack: TRACK_A,
      queue: [TRACK_A, TRACK_B, TRACK_C],
      queueIndex: 0,
    });
  });

  it("moves upcoming tracks via reorder buttons and saves the queue as a new playlist", async () => {
    const moveSpy = vi.spyOn(commands, "queueMove").mockResolvedValue({ status: "ok", data: null });
    const createSpy = vi.spyOn(commands, "createPlaylist").mockResolvedValue({
      status: "ok",
      data: {
        uuid: "pl-queue-1",
        title: "Ma file Hi-Res",
        description: "Créée depuis la file d'attente TideNative",
        creator: "Moi",
        number_of_tracks: 0,
        image_id: null,
      },
    });
    const addSpy = vi
      .spyOn(commands, "addTrackToPlaylist")
      .mockResolvedValue({ status: "ok", data: null });

    render(
      <TooltipProvider>
        <QueuePanel />
      </TooltipProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Monter Track Gamma" }));
    expect(moveSpy).toHaveBeenCalledWith(2, 1);

    fireEvent.click(screen.getByRole("button", { name: "Enregistrer la file en playlist" }));
    const input = screen.getByLabelText("Nom de la nouvelle playlist");
    fireEvent.change(input, { target: { value: "Ma file Hi-Res" } });
    fireEvent.click(screen.getByRole("button", { name: "Créer" }));

    await waitFor(() => expect(createSpy).toHaveBeenCalledWith("Ma file Hi-Res", expect.any(String)));
    await waitFor(() => expect(addSpy).toHaveBeenCalledTimes(3));
    expect(useLibraryStore.getState().playlists[0]?.uuid).toBe("pl-queue-1");
    expect(useLibraryStore.getState().playlists[0]?.number_of_tracks).toBe(3);

    moveSpy.mockRestore();
    createSpy.mockRestore();
    addSpy.mockRestore();
  });
});
