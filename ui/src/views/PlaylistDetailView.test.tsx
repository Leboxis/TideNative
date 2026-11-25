import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { type PlaylistDetailData, commands } from "@/bindings";
import { ImportPlaylistDialog } from "@/components/modals/ImportPlaylistDialog";
import { QueuePanel } from "@/components/player/QueuePanel";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";
import { PlaylistDetailView } from "./PlaylistDetailView";

const SAMPLE_PLAYLIST: PlaylistDetailData = {
  playlist: {
    uuid: "pl-uuid-1",
    title: "Ma sélection",
    description: "Ambiance nocturne",
    creator: "Moi",
    number_of_tracks: 2,
    image_id: null,
  },
  tracks: [
    {
      id: 1,
      title: "First Track",
      artist_name: "Artist A",
      artist_id: 10,
      album_title: "Album A",
      album_id: 100,
      duration_secs: 200,
      audio_quality: "LOSSLESS",
      cover_id: null,
      explicit: false,
    },
    {
      id: 2,
      title: "Second Track",
      artist_name: "Artist B",
      artist_id: 20,
      album_title: "Album B",
      album_id: 200,
      duration_secs: 240,
      audio_quality: "LOSSLESS",
      cover_id: null,
      explicit: false,
    },
  ],
  tidal_indices: [0, 1],
};

describe("PlaylistDetailView, ImportPlaylistDialog & QueuePanel Autoplay", () => {
  beforeEach(() => {
    useLibraryStore.setState({
      playlists: [SAMPLE_PLAYLIST.playlist],
      playlistFolders: [],
    });
    useUiStore.setState({
      route: { name: "playlist", playlistUuid: "pl-uuid-1" },
      history: [{ name: "playlist", playlistUuid: "pl-uuid-1" }],
      historyIndex: 0,
      importPlaylistOpen: false,
      queueOpen: false,
    });
  });

  it("edits playlist title and description via updatePlaylist", async () => {
    const getSpy = vi
      .spyOn(commands, "getPlaylistTracks")
      .mockResolvedValue({ status: "ok", data: SAMPLE_PLAYLIST });
    const updateSpy = vi.spyOn(commands, "updatePlaylist").mockResolvedValue({
      status: "ok",
      data: {
        ...SAMPLE_PLAYLIST.playlist,
        title: "Sélection mise à jour",
        description: "Nouvelle desc",
      },
    });

    render(<PlaylistDetailView playlistUuid="pl-uuid-1" />);
    const editBtn = await screen.findByRole("button", { name: /modifier/i });
    fireEvent.click(editBtn);

    const titleInput = screen.getByLabelText(/^nom$/i);
    const descInput = screen.getByLabelText(/description/i);
    fireEvent.change(titleInput, { target: { value: "Sélection mise à jour" } });
    fireEvent.change(descInput, { target: { value: "Nouvelle desc" } });
    fireEvent.click(screen.getByRole("button", { name: /enregistrer/i }));

    await waitFor(() =>
      expect(updateSpy).toHaveBeenCalledWith(
        "pl-uuid-1",
        "Sélection mise à jour",
        "Nouvelle desc",
      ),
    );
    expect(await screen.findByText("Sélection mise à jour")).toBeInTheDocument();
    expect(useLibraryStore.getState().playlists[0].title).toBe("Sélection mise à jour");

    getSpy.mockRestore();
    updateSpy.mockRestore();
  });

  it("reorders tracks in reorder mode via movePlaylistTrack", async () => {
    const getSpy = vi
      .spyOn(commands, "getPlaylistTracks")
      .mockResolvedValue({ status: "ok", data: SAMPLE_PLAYLIST });
    const moveSpy = vi
      .spyOn(commands, "movePlaylistTrack")
      .mockResolvedValue({ status: "ok", data: null });

    render(<PlaylistDetailView playlistUuid="pl-uuid-1" />);
    const reorderBtn = await screen.findByRole("button", { name: /réorganiser/i });
    fireEvent.click(reorderBtn);

    const moveUpBtn = screen.getByRole("button", { name: /monter second track/i });
    fireEvent.click(moveUpBtn);

    await waitFor(() => expect(moveSpy).toHaveBeenCalledWith("pl-uuid-1", 1, 0));

    getSpy.mockRestore();
    moveSpy.mockRestore();
  });

  it("duplicates a playlist and navigates to the new copy", async () => {
    const getSpy = vi
      .spyOn(commands, "getPlaylistTracks")
      .mockResolvedValue({ status: "ok", data: SAMPLE_PLAYLIST });
    const dupSpy = vi.spyOn(commands, "duplicatePlaylist").mockResolvedValue({
      status: "ok",
      data: {
        ...SAMPLE_PLAYLIST.playlist,
        uuid: "pl-uuid-copy",
        title: "Ma sélection (copie)",
      },
    });

    render(<PlaylistDetailView playlistUuid="pl-uuid-1" />);
    const dupBtn = await screen.findByRole("button", { name: /dupliquer/i });
    fireEvent.click(dupBtn);

    await waitFor(() => expect(dupSpy).toHaveBeenCalledWith("pl-uuid-1", null));
    expect(useUiStore.getState().route).toEqual({
      name: "playlist",
      playlistUuid: "pl-uuid-copy",
    });

    getSpy.mockRestore();
    dupSpy.mockRestore();
  });

  it("imports a playlist from CSV content via ImportPlaylistDialog", async () => {
    useUiStore.setState({ importPlaylistOpen: true });
    const importSpy = vi.spyOn(commands, "importPlaylist").mockResolvedValue({
      status: "ok",
      data: {
        playlist: {
          uuid: "pl-imported",
          title: "Import CSV",
          description: "Importée via TideNative",
          creator: "Moi",
          number_of_tracks: 1,
          image_id: null,
        },
        imported_count: 1,
        unmatched: [],
      },
    });

    render(<ImportPlaylistDialog />);
    fireEvent.change(screen.getByLabelText(/nom de la playlist/i), {
      target: { value: "Import CSV" },
    });
    fireEvent.change(screen.getByLabelText(/contenu csv/i), {
      target: { value: "title,artist,album,track_id\nDigital Love,Daft Punk,Discovery,555" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^importer$/i }));

    await waitFor(() =>
      expect(importSpy).toHaveBeenCalledWith(
        "Import CSV",
        "title,artist,album,track_id\nDigital Love,Daft Punk,Discovery,555",
        "Csv",
      ),
    );
    expect(useUiStore.getState().route).toEqual({
      name: "playlist",
      playlistUuid: "pl-imported",
    });

    importSpy.mockRestore();
  });

  it("toggles Découverte infinie (autoplay_radio) directly from QueuePanel", async () => {
    useUiStore.setState({ queueOpen: true });
    usePlaybackStore.setState({
      queue: [],
      queueIndex: null,
      currentTrack: null,
    });
    const prefSpy = vi
      .spyOn(commands, "updatePreferences")
      .mockResolvedValue({ status: "ok", data: null });

    render(<QueuePanel />);
    const sw = screen.getByRole("switch", { name: /découverte infinie/i });
    fireEvent.click(sw);

    await waitFor(() =>
      expect(prefSpy).toHaveBeenCalledWith(expect.objectContaining({ autoplay_radio: false })),
    );
    prefSpy.mockRestore();
  });
});
