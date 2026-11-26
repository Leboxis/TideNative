import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { commands } from "@/bindings";
import { AudioSettingsView } from "./AudioSettingsView";

describe("AudioSettingsView", () => {
  it("sends updated AppPreferences when toggling WASAPI Exclusive mode", async () => {
    const spy = vi
      .spyOn(commands, "updatePreferences")
      .mockResolvedValue({ status: "ok", data: null });

    render(<AudioSettingsView />);
    const sw = screen.getByRole("switch", { name: /mode exclusif wasapi/i });
    fireEvent.click(sw);

    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith(expect.objectContaining({ exclusive_mode: false })),
    );
    spy.mockRestore();
  });

  it("allows selecting the application font (season-mix, DM Sans, Plus Jakarta Sans)", async () => {
    const spy = vi
      .spyOn(commands, "updatePreferences")
      .mockResolvedValue({ status: "ok", data: null });

    render(<AudioSettingsView />);
    const seasonMixBtn = screen.getByRole("button", { name: /season-mix/i });
    fireEvent.click(seasonMixBtn);

    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith(expect.objectContaining({ ui_font: "SeasonMix" })),
    );
    expect(document.documentElement.dataset.uiFont).toBe("SeasonMix");
    spy.mockRestore();
  });

  it("checks for Velopack updates and shows download action when a new version is found", async () => {
    const statusSpy = vi.spyOn(commands, "getAppUpdateStatus").mockResolvedValue({
      status: "ok",
      data: {
        current_version: "0.1.0",
        is_installed: true,
        update_available: false,
        pending_restart: false,
        available_version: null,
        release_notes: null,
        is_delta: false,
        download_size_bytes: null,
        source_url: "https://github.com/Leboxis/TideNative",
      },
    });
    const checkSpy = vi.spyOn(commands, "checkForUpdates").mockResolvedValue({
      status: "ok",
      data: {
        current_version: "0.1.0",
        is_installed: true,
        update_available: true,
        pending_restart: false,
        available_version: "0.2.0",
        release_notes: "Correctifs audio",
        is_delta: true,
        download_size_bytes: 2_621_440,
        source_url: "https://github.com/Leboxis/TideNative",
      },
    });

    render(<AudioSettingsView />);
    const checkBtn = screen.getByRole("button", { name: /rechercher une mise à jour/i });
    fireEvent.click(checkBtn);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: /télécharger v0\.2\.0/i })).toBeTruthy(),
    );
    statusSpy.mockRestore();
    checkSpy.mockRestore();
  });
});
