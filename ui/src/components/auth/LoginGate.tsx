import { ShieldCheck, Waves } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { Button } from "@/components/ui/button";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlaybackStore } from "@/stores/playbackStore";

export function LoginGate() {
  const [loading, setLoading] = useState(false);
  const hydratePlayback = usePlaybackStore((s) => s.hydrate);
  const hydrateLibrary = useLibraryStore((s) => s.hydrate);

  const handleLogin = async () => {
    setLoading(true);
    try {
      const res = await commands.openTidalLogin();
      if (res.status === "ok" && res.data) {
        hydratePlayback(res.data);
        hydrateLibrary(res.data);
        toast.success("Connecté à votre compte TIDAL.");
      } else if (res.status === "error") {
        toast.error(res.error);
      }
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex flex-1 items-center justify-center p-6">
      <div className="card-oled w-full max-w-md rounded-3xl p-8 text-center">
        <div className="mx-auto mb-5 flex h-14 w-14 items-center justify-center rounded-2xl border border-[#33F3FF]/30 bg-[#33F3FF]/10 text-[#33F3FF]">
          <Waves className="h-7 w-7" />
        </div>
        <h1 className="text-xl font-semibold tracking-tight text-white">
          Connexion à TIDAL
        </h1>
        <p className="mt-2 text-sm leading-relaxed text-white/55">
          Connectez-vous directement sur le site officiel{" "}
          <span className="text-white/85 font-medium">listen.tidal.com</span> dans une
          fenêtre WebView2 isolée. Vos identifiants restent chiffrés dans le Gestionnaire
          d&apos;identification Windows.
        </p>

        <Button
          size="lg"
          disabled={loading}
          onClick={handleLogin}
          className="mt-6 w-full"
        >
          {loading ? "Fenêtre TIDAL ouverte…" : "Se connecter avec TIDAL"}
        </Button>

        <div className="mt-5 flex items-center justify-center gap-2 text-xs text-white/40">
          <ShieldCheck className="h-3.5 w-3.5 text-[#30D158]" />
          <span>FLAC Hi-Res 24-bit / 192 kHz • WASAPI Exclusif Bit-Perfect</span>
        </div>
      </div>
    </div>
  );
}
