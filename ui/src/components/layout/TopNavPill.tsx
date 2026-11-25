import {
  Activity,
  ChevronLeft,
  ChevronRight,
  Compass,
  Copy,
  Headphones,
  Heart,
  Library,
  Minus,
  Search,
  Settings,
  Square,
  X,
} from "lucide-react";
import { motion } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { commands } from "@/bindings";
import { ActionTooltip } from "@/components/ui/tooltip";
import { type AppRoute, useUiStore } from "@/stores/uiStore";

const NAV_ITEMS: { id: AppRoute["name"]; label: string; icon: typeof Headphones }[] = [
  { id: "discover", label: "Écouter", icon: Headphones },
  { id: "explore", label: "Explorer", icon: Compass },
  { id: "favorites", label: "Favoris", icon: Heart },
  { id: "collection", label: "Collection", icon: Library },
  { id: "activity", label: "Activité", icon: Activity },
  { id: "settings", label: "Paramètres", icon: Settings },
];

export function TopNavPill() {
  const route = useUiStore((s) => s.route);
  const navigate = useUiStore((s) => s.navigate);
  const goBack = useUiStore((s) => s.goBack);
  const goForward = useUiStore((s) => s.goForward);
  const canBack = useUiStore((s) => s.historyIndex > 0);
  const canForward = useUiStore((s) => s.historyIndex < s.history.length - 1);
  const commandOpen = useUiStore((s) => s.commandOpen);
  const setCommandOpen = useUiStore((s) => s.setCommandOpen);

  const [maximized, setMaximized] = useState(false);
  const [searchPulse, setSearchPulse] = useState(false);
  const pulseTimerRef = useRef<number | null>(null);
  const resizeTimerRef = useRef<number | null>(null);

  const syncMaximized = useCallback(() => {
    commands
      .isWindowMaximized()
      .then((res) => {
        if (res.status === "ok") setMaximized(res.data);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    syncMaximized();
    const onResize = () => {
      if (resizeTimerRef.current !== null) {
        window.clearTimeout(resizeTimerRef.current);
      }
      resizeTimerRef.current = window.setTimeout(() => {
        resizeTimerRef.current = null;
        syncMaximized();
      }, 100);
    };
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("resize", onResize);
      if (pulseTimerRef.current !== null) {
        window.clearTimeout(pulseTimerRef.current);
      }
      if (resizeTimerRef.current !== null) {
        window.clearTimeout(resizeTimerRef.current);
      }
    };
  }, [syncMaximized]);

  const handleToggleMax = async () => {
    const res = await commands.toggleMaximizeWindow();
    if (res.status === "ok") setMaximized(res.data);
  };

  const handleHeaderDoubleClick = (e: React.MouseEvent<HTMLElement>) => {
    const target = e.target as HTMLElement;
    if (target.hasAttribute("data-tauri-drag-region")) {
      void handleToggleMax();
    }
  };

  const activePrimary: AppRoute["name"] = route.name;

  return (
    <header
      data-tauri-drag-region
      onDoubleClick={handleHeaderDoubleClick}
      className="relative z-30 flex h-14 shrink-0 items-center select-none"
    >
      <div
        data-tauri-drag-region
        className="grid h-full w-full grid-cols-[1fr_auto_1fr] items-center gap-3 px-4"
      >
        {/* Left: History navigation + Integrated quick search pill */}
        <div data-tauri-drag-region className="flex h-full items-center gap-2 min-w-0">
          <div className="flex items-center gap-1 shrink-0">
            <ActionTooltip label="Précédent" shortcut="Alt+←" side="bottom">
              <button
                type="button"
                aria-label="Précédent"
                disabled={!canBack}
                onClick={goBack}
                className="flex h-8 w-8 items-center justify-center rounded-full border border-white/[0.08] bg-white/[0.04] text-white/75 hover:bg-white/10 hover:text-white disabled:opacity-30 disabled:pointer-events-none transition-colors cursor-pointer"
              >
                <ChevronLeft className="h-4 w-4" />
              </button>
            </ActionTooltip>
            <ActionTooltip label="Suivant" shortcut="Alt+→" side="bottom">
              <button
                type="button"
                aria-label="Suivant"
                disabled={!canForward}
                onClick={goForward}
                className="flex h-8 w-8 items-center justify-center rounded-full border border-white/[0.08] bg-white/[0.04] text-white/75 hover:bg-white/10 hover:text-white disabled:opacity-30 disabled:pointer-events-none transition-colors cursor-pointer"
              >
                <ChevronRight className="h-4 w-4" />
              </button>
            </ActionTooltip>
          </div>

          <motion.button
            type="button"
            aria-label="Rechercher (Ctrl+K)"
            onClick={() => {
              setSearchPulse(true);
              if (pulseTimerRef.current !== null) {
                window.clearTimeout(pulseTimerRef.current);
              }
              pulseTimerRef.current = window.setTimeout(() => {
                setSearchPulse(false);
                pulseTimerRef.current = null;
              }, 260);
              setCommandOpen(true);
            }}
            initial={false}
            animate={{ scale: commandOpen || searchPulse ? 1.02 : 1 }}
            whileTap={{ scale: 0.98 }}
            transition={{ type: "spring", stiffness: 440, damping: 30, mass: 0.65 }}
            className={`glass-surface relative flex h-8 w-full min-w-[112px] max-w-[210px] items-center justify-between gap-2 overflow-hidden rounded-full px-3 text-xs transition-colors cursor-pointer ${
              commandOpen || searchPulse
                ? "border-[#33F3FF]/45 bg-white/[0.10] text-white"
                : "text-white/65 hover:border-white/20 hover:bg-white/[0.08] hover:text-white"
            }`}
          >
            <span className="flex items-center gap-2 truncate">
              <Search
                className={`h-3.5 w-3.5 shrink-0 transition-colors ${
                  commandOpen || searchPulse ? "text-[#33F3FF]" : "text-white/60"
                }`}
              />
              <span className="truncate">Rechercher…</span>
            </span>
            <kbd className="hidden xl:inline-flex shrink-0 rounded-md border border-white/10 bg-white/[0.08] px-1.5 py-0.5 text-[10px] font-mono text-white/65">
              Ctrl K
            </kbd>
          </motion.button>

          <div data-tauri-drag-region className="h-full flex-1 min-w-2" />
        </div>

        {/* Center: Floating glass navigation pill */}
        <nav
          aria-label="Navigation principale"
          className="glass-surface flex items-center gap-0.5 rounded-full p-1 shrink-0 justify-self-center"
        >
          {NAV_ITEMS.map((item) => {
            const Icon = item.icon;
            const isActive = activePrimary === item.id;
            return (
              <button
                key={item.id}
                type="button"
                aria-current={isActive ? "page" : undefined}
                onClick={() => navigate({ name: item.id } as AppRoute)}
                className={`relative flex items-center gap-1.5 rounded-full px-3 xl:px-3.5 py-1.5 text-xs font-medium transition-colors duration-150 cursor-pointer ${
                  isActive ? "text-[#050507]" : "text-white/75 hover:text-white"
                }`}
              >
                {isActive && (
                  <motion.span
                    layoutId="nav-pill-active"
                    className="absolute inset-0 rounded-full bg-white shadow-sm"
                    transition={{ type: "spring", stiffness: 420, damping: 34, mass: 0.7 }}
                  />
                )}
                <Icon className="relative z-10 h-3.5 w-3.5" />
                <span className="relative z-10 font-semibold">{item.label}</span>
              </button>
            );
          })}
        </nav>

        {/* Right: Integrated frameless window controls */}
        <div data-tauri-drag-region className="flex h-full items-center justify-end gap-2 min-w-0">
          <div data-tauri-drag-region className="h-full flex-1" />

          <div className="flex items-center gap-0.5 shrink-0">
            <ActionTooltip label="Réduire" side="bottom">
              <button
                type="button"
                aria-label="Réduire"
                onClick={() => void commands.minimizeWindow()}
                className="flex h-8 w-8 items-center justify-center rounded-lg text-white/70 hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
              >
                <Minus className="h-3.5 w-3.5" />
              </button>
            </ActionTooltip>
            <ActionTooltip label={maximized ? "Restaurer" : "Agrandir"} side="bottom">
              <button
                type="button"
                aria-label={maximized ? "Restaurer" : "Agrandir"}
                onClick={() => void handleToggleMax()}
                className="flex h-8 w-8 items-center justify-center rounded-lg text-white/70 hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
              >
                {maximized ? <Copy className="h-3 w-3" /> : <Square className="h-3 w-3" />}
              </button>
            </ActionTooltip>
            <ActionTooltip label="Fermer" side="bottom">
              <button
                type="button"
                aria-label="Fermer"
                onClick={() => void commands.closeMainWindow()}
                className="flex h-8 w-8 items-center justify-center rounded-lg text-white/70 hover:bg-[#ff453a] hover:text-white transition-colors cursor-pointer"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            </ActionTooltip>
          </div>
        </div>
      </div>
    </header>
  );
}
