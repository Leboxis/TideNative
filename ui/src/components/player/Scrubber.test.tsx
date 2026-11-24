import { render } from "@testing-library/react";
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
});
