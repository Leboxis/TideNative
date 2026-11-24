import { useEffect, useState } from "react";
import { type TrackCredits, commands } from "@/bindings";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useUiStore } from "@/stores/uiStore";

export function CreditsDialog() {
  const track = useUiStore((s) => s.creditsTrack);
  const openCredits = useUiStore((s) => s.openCredits);
  const [credits, setCredits] = useState<TrackCredits | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!track) {
      setCredits(null);
      return;
    }
    let active = true;
    setLoading(true);
    void commands.getCredits(track.id).then((res) => {
      if (!active) return;
      setLoading(false);
      if (res.status === "ok") setCredits(res.data);
    });
    return () => {
      active = false;
    };
  }, [track]);

  return (
    <Dialog open={track !== null} onOpenChange={(o) => !o && openCredits(null)}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>Crédits</DialogTitle>
          {track && (
            <DialogDescription>
              {track.title} — {track.artist_name}
            </DialogDescription>
          )}
        </DialogHeader>

        <div className="max-h-96 space-y-4 overflow-y-auto pr-1">
          {loading ? (
            <p className="py-6 text-center text-xs text-white/45">Chargement des crédits…</p>
          ) : !credits || credits.groups.length === 0 ? (
            <p className="py-6 text-center text-xs text-white/45">
              Aucun crédit détaillé disponible pour ce titre.
            </p>
          ) : (
            credits.groups.map((group) => (
              <div key={group.role} className="space-y-1">
                <p className="text-[11px] font-semibold uppercase tracking-wider text-white/45">
                  {group.role}
                </p>
                <p className="text-sm text-white/90">{group.contributors.join(", ")}</p>
              </div>
            ))
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
