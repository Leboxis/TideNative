import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "@/bindings";
import { telemetryRuntime } from "@/lib/telemetry";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";
import AudioPlayer, { CompactPlayer, formatTime } from "./CompactPlayer";

describe("CompactPlayer / AudioPlayer", () => {
  beforeEach(() => {
    useUiStore.setState({ compactMode: false });
    usePlaybackStore.setState({
      playbackState: "Playing",
      durationMs: 235_000,
      currentTrack: {
        id: 42,
        title: "TOKYO MACHINE",
        artist_name: "NEFFEX",
        artist_id: 7,
        album_id: 9,
        album_title: "THE ACT",
        cover_id: "11111111-2222-3333-4444-555555555555",
        duration_secs: 235,
        audio_quality: "HI_RES_LOSSLESS",
        explicit: false,
      },
    });
    telemetryRuntime.seedPosition(26_000, 235_000);
  });

  it("formats seconds as M:SS matching compact player display", () => {
    expect(formatTime(26)).toBe("0:26");
    expect(formatTime(235)).toBe("3:55");
    expect(formatTime(0)).toBe("0:00");
  });

  it("renders current TideNative track, timestamps, and dispatches transport commands", () => {
    const playSpy = vi
      .spyOn(commands, "togglePlayPause")
      .mockResolvedValue({ status: "ok", data: null });
    const prevSpy = vi
      .spyOn(commands, "prevTrack")
      .mockResolvedValue({ status: "ok", data: null });
    const nextSpy = vi
      .spyOn(commands, "nextTrack")
      .mockResolvedValue({ status: "ok", data: null });
    const shuffleSpy = vi
      .spyOn(commands, "setShuffle")
      .mockResolvedValue({ status: "ok", data: null });
    const repeatSpy = vi
      .spyOn(commands, "cycleRepeat")
      .mockResolvedValue({ status: "ok", data: "All" });

    render(<CompactPlayer />);

    expect(screen.getByText("TOKYO MACHINE")).toBeInTheDocument();
    expect(screen.getByText("NEFFEX")).toBeInTheDocument();
    expect(screen.getByText("0:26")).toBeInTheDocument();
    expect(screen.getByText("3:55")).toBeInTheDocument();

    fireEvent.click(screen.getByLabelText("Pause"));
    expect(playSpy).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByLabelText("Piste précédente"));
    expect(prevSpy).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByLabelText("Piste suivante"));
    expect(nextSpy).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByLabelText("Aléatoire"));
    expect(shuffleSpy).toHaveBeenCalledWith(true);

    fireEvent.click(screen.getByLabelText("Répétition"));
    expect(repeatSpy).toHaveBeenCalledTimes(1);

    playSpy.mockRestore();
    prevSpy.mockRestore();
    nextSpy.mockRestore();
    shuffleSpy.mockRestore();
    repeatSpy.mockRestore();
  });

  it("returns null in standalone src mode when src is empty", () => {
    const { container } = render(<AudioPlayer src="" />);
    expect(container.firstChild).toBeNull();
  });

  it("quits the app from the discreet close button and exits compact mode from the expand button", () => {
    useUiStore.setState({ compactMode: true });
    const quitSpy = vi
      .spyOn(commands, "quitApp")
      .mockResolvedValue({ status: "ok", data: null });
    const compactSpy = vi
      .spyOn(commands, "setCompactMode")
      .mockResolvedValue({ status: "ok", data: true });

    render(<CompactPlayer />);

    fireEvent.click(screen.getByLabelText("Fermer l'application"));
    expect(quitSpy).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByLabelText("Quitter le mode compact"));
    expect(compactSpy).toHaveBeenCalledWith(false);
    expect(useUiStore.getState().compactMode).toBe(false);

    quitSpy.mockRestore();
    compactSpy.mockRestore();
  });

  it("toggles Always on Top pin state via setAlwaysOnTop command", () => {
    useUiStore.setState({ compactMode: true, compactAlwaysOnTop: true });
    const pinSpy = vi
      .spyOn(commands, "setAlwaysOnTop")
      .mockResolvedValue({ status: "ok", data: false });

    render(<CompactPlayer />);

    const unpinBtn = screen.getByRole("button", { name: "Détacher du premier plan" });
    expect(unpinBtn).toHaveAttribute("aria-pressed", "true");

    fireEvent.click(unpinBtn);
    expect(pinSpy).toHaveBeenCalledWith(false);
    expect(useUiStore.getState().compactAlwaysOnTop).toBe(false);

    const pinBtn = screen.getByRole("button", { name: "Épingler au premier plan" });
    fireEvent.click(pinBtn);
    expect(pinSpy).toHaveBeenLastCalledWith(true);
    expect(useUiStore.getState().compactAlwaysOnTop).toBe(true);

    pinSpy.mockRestore();
  });
});
