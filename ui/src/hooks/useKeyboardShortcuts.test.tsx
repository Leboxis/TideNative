import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "@/bindings";
import { telemetryRuntime } from "@/lib/telemetry";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";
import { useKeyboardShortcuts } from "./useKeyboardShortcuts";

function Harness() {
  useKeyboardShortcuts();
  return (
    <div>
      <input aria-label="search-box" />
      <button type="button">Action</button>
    </div>
  );
}

describe("useKeyboardShortcuts", () => {
  beforeEach(() => {
    useUiStore.setState({
      route: { name: "discover" },
      history: [{ name: "discover" }],
      historyIndex: 0,
      commandOpen: false,
      shortcutsOpen: false,
      compactMode: false,
    });
    usePlaybackStore.setState((s) => ({
      durationMs: 240_000,
      preferences: { ...s.preferences, volume: 0.5, bit_perfect_lock: false },
    }));
    telemetryRuntime.seedPosition(30_000, 240_000);
  });

  it("toggles playback on Space outside inputs/buttons, and ignores Space on inputs or focused buttons", () => {
    const spy = vi.spyOn(commands, "togglePlayPause").mockResolvedValue({ status: "ok", data: null });
    render(<Harness />);

    const input = screen.getByLabelText("search-box");
    fireEvent.keyDown(input, { code: "Space", key: " " });
    expect(spy).not.toHaveBeenCalled();

    const btn = screen.getByRole("button", { name: "Action" });
    fireEvent.keyDown(btn, { code: "Space", key: " " });
    expect(spy).not.toHaveBeenCalled();

    fireEvent.keyDown(document.body, { code: "Space", key: " " });
    expect(spy).toHaveBeenCalledTimes(1);
    spy.mockRestore();
  });

  it("toggles compact mode on C and exits compact mode on Escape", () => {
    const compactSpy = vi
      .spyOn(commands, "setCompactMode")
      .mockResolvedValue({ status: "ok", data: true });
    render(<Harness />);

    fireEvent.keyDown(document.body, { key: "c" });
    expect(compactSpy).toHaveBeenCalledWith(true);

    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(compactSpy).toHaveBeenLastCalledWith(false);
    compactSpy.mockRestore();
  });

  it("supports arrow seek, arrow volume, slash search, question shortcuts dialog, and mouse back/forward", () => {
    const seekSpy = vi.spyOn(commands, "seekMs").mockResolvedValue({ status: "ok", data: null });
    const volSpy = vi.spyOn(commands, "setVolume").mockResolvedValue({ status: "ok", data: null });
    render(<Harness />);

    fireEvent.keyDown(document.body, { key: "ArrowRight" });
    expect(seekSpy).toHaveBeenLastCalledWith(40_000);

    fireEvent.keyDown(document.body, { key: "ArrowLeft" });
    expect(seekSpy).toHaveBeenLastCalledWith(30_000);

    fireEvent.keyDown(document.body, { key: "ArrowUp" });
    expect(volSpy).toHaveBeenLastCalledWith(0.55);

    fireEvent.keyDown(document.body, { key: "ArrowDown" });
    expect(volSpy).toHaveBeenLastCalledWith(0.45);

    fireEvent.keyDown(document.body, { key: "/" });
    expect(useUiStore.getState().commandOpen).toBe(true);

    fireEvent.keyDown(document.body, { key: "?" });
    expect(useUiStore.getState().shortcutsOpen).toBe(true);

    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(useUiStore.getState().shortcutsOpen).toBe(false);

    useUiStore.getState().navigate({ name: "explore" });
    expect(useUiStore.getState().route).toEqual({ name: "explore" });

    fireEvent.mouseUp(window, { button: 3 });
    expect(useUiStore.getState().route).toEqual({ name: "discover" });

    fireEvent.mouseUp(window, { button: 4 });
    expect(useUiStore.getState().route).toEqual({ name: "explore" });

    seekSpy.mockRestore();
    volSpy.mockRestore();
  });
});
