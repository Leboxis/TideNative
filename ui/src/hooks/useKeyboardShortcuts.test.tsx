import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { commands } from "@/bindings";
import { useKeyboardShortcuts } from "./useKeyboardShortcuts";

function Harness() {
  useKeyboardShortcuts();
  return (
    <div>
      <input aria-label="search-box" />
    </div>
  );
}

describe("useKeyboardShortcuts", () => {
  it("toggles playback on Space outside inputs, and ignores Space while typing", () => {
    const spy = vi.spyOn(commands, "togglePlayPause").mockResolvedValue({ status: "ok", data: null });
    render(<Harness />);

    const input = screen.getByLabelText("search-box");
    fireEvent.keyDown(input, { code: "Space", key: " " });
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
});
