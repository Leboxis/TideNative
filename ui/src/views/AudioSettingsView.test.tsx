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
});
