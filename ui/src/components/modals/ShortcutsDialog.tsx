import { Keyboard } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useUiStore } from "@/stores/uiStore";

const SHORTCUT_GROUPS: {
  title: string;
  items: { keys: string[]; label: string }[];
}[] = [
  {
    title: "Lecture & Audio",
    items: [
      { keys: ["Espace"], label: "Lecture / Pause" },
      { keys: ["←", "→"], label: "Reculer / Avancer de 10 s" },
      { keys: ["Ctrl", "← / →"], label: "Piste précédente / suivante" },
      { keys: ["↑", "↓"], label: "Volume ± 5 %" },
      { keys: ["M"], label: "Muet / Rétablir le son" },
    ],
  },
  {
    title: "Navigation & Vues",
    items: [
      { keys: ["Ctrl", "K"], label: "Recherche rapide (ou /)" },
      { keys: ["Alt", "← / →"], label: "Page précédente / suivante (ou boutons souris)" },
      { keys: ["N"], label: "Plein écran En cours de lecture" },
      { keys: ["Q"], label: "Ouvrir / Fermer la file d'attente" },
      { keys: ["C"], label: "Mode compact" },
      { keys: ["?"], label: "Afficher ce mémo" },
      { keys: ["Échap"], label: "Fermer la fenêtre / le panneau actif" },
    ],
  },
];

export function ShortcutsDialog() {
  const open = useUiStore((s) => s.shortcutsOpen);
  const setOpen = useUiStore((s) => s.setShortcutsOpen);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Keyboard className="h-4 w-4 text-[#33F3FF]" />
            Raccourcis clavier & souris
          </DialogTitle>
          <DialogDescription>
            Pilotez TideNative instantanément sans quitter le clavier.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-5">
          {SHORTCUT_GROUPS.map((group) => (
            <div key={group.title} className="space-y-2">
              <h3 className="text-[11px] font-semibold uppercase tracking-wider text-white/60">
                {group.title}
              </h3>
              <div className="divide-y divide-white/[0.06] rounded-xl border border-white/[0.08] bg-white/[0.02]">
                {group.items.map((item) => (
                  <div
                    key={item.label}
                    className="flex items-center justify-between gap-4 px-3.5 py-2 text-xs"
                  >
                    <span className="text-white/85">{item.label}</span>
                    <div className="flex shrink-0 items-center gap-1">
                      {item.keys.map((key) => (
                        <kbd
                          key={key}
                          className="rounded-md border border-white/15 bg-white/[0.08] px-1.5 py-0.5 font-mono text-[10px] text-white/90"
                        >
                          {key}
                        </kbd>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
