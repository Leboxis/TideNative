import { describe, expect, it } from "vitest";
import { coverUrl } from "./covers";
import { decodeTelemetryFrame } from "./telemetry";

describe("telemetry binary decoder", () => {
  it("decodes 44-byte little-endian telemetry frame without allocating new levels array", () => {
    const buf = new ArrayBuffer(44);
    const view = new DataView(buf);
    [0.25, 0.5, 0.75, 0.1, 0.9].forEach((v, i) => view.setFloat32(i * 4, v, true));
    view.setBigUint64(20, 61_500n, true);
    view.setBigUint64(28, 210_000n, true);
    view.setBigUint64(36, 12_000n, true);

    const levels = new Float32Array(5);
    const res = decodeTelemetryFrame(buf, levels);

    expect(levels[0]).toBeCloseTo(0.25);
    expect(levels[2]).toBeCloseTo(0.75);
    expect(levels[4]).toBeCloseTo(0.9);
    expect(res).toEqual({ positionMs: 61_500, durationMs: 210_000, bufferedMs: 12_000 });
  });

  it("returns null on truncated frames", () => {
    const levels = new Float32Array(5);
    expect(decodeTelemetryFrame(new ArrayBuffer(12), levels)).toBeNull();
  });
});

describe("coverUrl", () => {
  it("encodes UUIDs and full OpenAPI v2 HTTPS URLs cleanly", () => {
    expect(coverUrl("abc-123", 640)).toBe("http://tidal-cover.localhost/abc-123/640");
    expect(coverUrl("https://resources.tidal.com/images/a/b.jpg", 320)).toBe(
      "http://tidal-cover.localhost/https%3A%2F%2Fresources.tidal.com%2Fimages%2Fa%2Fb.jpg/320",
    );
    expect(coverUrl(null, 320)).toBeNull();
  });
});
