import { fireEvent, render, screen } from "@testing-library/react";
import { Profiler } from "react";
import { describe, expect, it, vi } from "vitest";
import { telemetryRuntime } from "@/lib/telemetry";
import { Scrubber } from "./Scrubber";

describe("Scrubber 120Hz zero-re-render guarantee", () => {
  it("does not trigger React re-renders across incoming 60Hz telemetry frames", () => {
    const onRender = vi.fn();
    render(
      <Profiler id="scrubber" onRender={onRender}>
        <Scrubber durationMs={240_000} onSeek={vi.fn()} />
      </Profiler>,
    );
    const initialRenders = onRender.mock.calls.length;

    for (let i = 1; i <= 15; i++) {
      telemetryRuntime.seedPosition(i * 1_000, 240_000);
    }

    expect(onRender.mock.calls.length).toBe(initialRenders);
  });

  it("supports keyboard seeking and keeps aria-valuenow synchronized", () => {
    const onSeek = vi.fn();
    telemetryRuntime.seedPosition(30_000, 240_000);
    render(<Scrubber durationMs={240_000} onSeek={onSeek} />);

    const slider = screen.getByRole("slider", { name: "Position de lecture" });
    expect(slider).toHaveAttribute("aria-valuenow", "30");

    fireEvent.keyDown(slider, { key: "ArrowRight" });
    expect(onSeek).toHaveBeenLastCalledWith(35_000);
    expect(slider).toHaveAttribute("aria-valuenow", "35");

    fireEvent.keyDown(slider, { key: "ArrowLeft", shiftKey: true });
    expect(onSeek).toHaveBeenLastCalledWith(25_000);
    expect(slider).toHaveAttribute("aria-valuenow", "25");

    fireEvent.keyDown(slider, { key: "Home" });
    expect(onSeek).toHaveBeenLastCalledWith(0);

    fireEvent.keyDown(slider, { key: "End" });
    expect(onSeek).toHaveBeenLastCalledWith(240_000);
  });

  it("cancels an interrupted pointer drag without dispatching onSeek", () => {
    const onSeek = vi.fn();
    telemetryRuntime.seedPosition(60_000, 240_000);
    render(<Scrubber durationMs={240_000} onSeek={onSeek} />);

    const slider = screen.getByRole("slider", { name: "Position de lecture" });
    vi.spyOn(slider, "getBoundingClientRect").mockReturnValue({
      x: 0,
      y: 0,
      left: 0,
      top: 0,
      right: 200,
      bottom: 20,
      width: 200,
      height: 20,
      toJSON: () => ({}),
    });

    fireEvent.pointerDown(slider, { clientX: 150, pointerId: 1 });
    fireEvent.pointerCancel(slider, { clientX: 150, pointerId: 1 });

    expect(onSeek).not.toHaveBeenCalled();
    expect(slider).toHaveAttribute("aria-valuenow", "60");
  });
});
