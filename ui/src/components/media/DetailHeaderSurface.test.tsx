import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import * as covers from "@/lib/covers";
import { DetailHeaderSurface } from "./DetailHeaderSurface";

describe("DetailHeaderSurface", () => {
  it("falls back cleanly when no coverId is provided", () => {
    render(
      <DetailHeaderSurface coverId={null}>
        <h1>Untitled Album</h1>
      </DetailHeaderSurface>,
    );

    expect(screen.getByText("Untitled Album")).toBeInTheDocument();
    const tintLayer = screen.getByTestId("detail-header-tint");
    expect(tintLayer).toHaveAttribute("data-tinted", "false");
    expect(tintLayer.style.backgroundImage).toContain("radial-gradient");
  });

  it("applies harmonized tint derived from extracted cover colors", async () => {
    const extractSpy = vi.spyOn(covers, "extractTrackRowColors").mockResolvedValue({
      colors: ["#FF5522", "#2288FF", "#33F3FF", "#FFD432"],
      titleColor: "#FF7744",
    });

    render(
      <DetailHeaderSurface coverId="cover-uuid-123">
        <h1>Random Access Memories</h1>
      </DetailHeaderSurface>,
    );

    await waitFor(() =>
      expect(screen.getByTestId("detail-header-tint")).toHaveAttribute("data-tinted", "true"),
    );

    const derived = covers.deriveHeaderTint({
      colors: ["#FF5522", "#2288FF", "#33F3FF", "#FFD432"],
      titleColor: "#FF7744",
    });
    expect(derived.primary).toMatch(/^#[0-9A-F]{6}$/);
    expect(derived.secondary).toMatch(/^#[0-9A-F]{6}$/);
    expect(derived.accent).toBe("#FF7744");

    extractSpy.mockRestore();
  });
});
