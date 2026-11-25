import type { ReactNode } from "react";
import { AmbientBackdrop } from "@/components/layout/AmbientBackdrop";
import { TopNavPill } from "@/components/layout/TopNavPill";
import { CompactPlayer } from "@/components/player/CompactPlayer";
import { PlayerBar } from "@/components/player/PlayerBar";
import { useUiStore } from "@/stores/uiStore";

interface AppShellProps {
  children: ReactNode;
  overlays?: ReactNode;
}

export function AppShell({ children, overlays }: AppShellProps) {
  const compactMode = useUiStore((s) => s.compactMode);

  if (compactMode) {
    return (
      <div
        data-tauri-drag-region
        className="relative flex h-screen w-screen items-center justify-center overflow-hidden bg-[#040406] text-[#f5f5f7]"
      >
        <AmbientBackdrop intensity="subtle" />
        <div className="relative z-10 h-full w-full">
          <CompactPlayer />
        </div>
      </div>
    );
  }

  return (
    <div className="relative flex h-screen w-screen flex-col overflow-hidden bg-[#040406] text-[#f5f5f7]">
      <AmbientBackdrop intensity="subtle" />

      <TopNavPill />

      <main className="relative z-10 flex-1 min-h-0 overflow-hidden px-6 pb-2">
        <div className="mx-auto flex h-full max-w-[1440px] flex-col overflow-y-auto pr-1">
          {children}
        </div>
      </main>

      <PlayerBar />
      {overlays}
    </div>
  );
}
