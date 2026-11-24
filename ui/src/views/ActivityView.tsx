import { Clock, Disc3, Flame, Play, Trash2, Users } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import {
  type ActivityPeriod,
  type ListeningActivityStats,
  commands,
} from "@/bindings";
import { CoverImage } from "@/components/media/CoverImage";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { usePlaybackStore } from "@/stores/playbackStore";
import { useUiStore } from "@/stores/uiStore";

function formatHours(totalSecs: number): string {
  const safe = Math.max(0, Math.floor(totalSecs));
  const hours = Math.floor(safe / 3600);
  const mins = Math.floor((safe % 3600) / 60);
  const secs = safe % 60;
  if (hours > 0) {
    return `${hours} h ${mins.toString().padStart(2, "0")} min ${secs.toString().padStart(2, "0")} s`;
  }
  if (mins > 0) {
    return `${mins} min ${secs.toString().padStart(2, "0")} s`;
  }
  return `${secs} s`;
}

export function ActivityView() {
  const lastfm = usePlaybackStore((s) => s.lastfm);
  const setLastFm = usePlaybackStore((s) => s.setLastFm);
  const playbackState = usePlaybackStore((s) => s.playbackState);
  const currentTrackId = usePlaybackStore((s) => s.currentTrack?.id ?? null);
  const navigate = useUiStore((s) => s.navigate);

  const [period, setPeriod] = useState<ActivityPeriod>("AllTime");
  const [stats, setStats] = useState<ListeningActivityStats | null>(null);
  const [lastfmModalOpen, setLastfmModalOpen] = useState(false);
  const [clearModalOpen, setClearModalOpen] = useState(false);

  const [apiKey, setApiKey] = useState("");
  const [apiSecret, setApiSecret] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const loadStats = useCallback(async (p: ActivityPeriod) => {
    const res = await commands.getActivityStats(p);
    if (res.status === "ok") setStats(res.data);
  }, []);

  useEffect(() => {
    void loadStats(period);
  }, [period, currentTrackId, playbackState, loadStats]);

  useEffect(() => {
    if (playbackState !== "Playing") return;
    const interval = window.setInterval(() => {
      void loadStats(period);
    }, 2000);
    return () => window.clearInterval(interval);
  }, [playbackState, period, loadStats]);

  const handleConnectLastFm = async (e: React.FormEvent) => {
    e.preventDefault();
    setSubmitting(true);
    const res = await commands.connectLastfm(apiKey, apiSecret, username, password);
    setSubmitting(false);
    if (res.status === "ok") {
      setLastFm(res.data);
      toast.success("Compte Last.fm connecté");
      setLastfmModalOpen(false);
      setPassword("");
    } else {
      toast.error(res.error);
    }
  };

  const handleDisconnectLastFm = async () => {
    const res = await commands.disconnectLastfm();
    if (res.status === "ok") {
      setLastFm(res.data);
      toast.info("Compte Last.fm déconnecté");
    }
  };

  const handleClearHistory = async () => {
    const res = await commands.clearActivityHistory();
    if (res.status === "ok") {
      setClearModalOpen(false);
      toast.success("Historique local effacé");
      void loadStats(period);
    } else {
      toast.error(res.error);
    }
  };

  return (
    <div className="space-y-6 pb-8">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-white">Mon Activité</h1>
          <p className="text-xs text-white/45 mt-0.5">
            Statistiques locales SQLite et scrobbling automatique Last.fm
          </p>
        </div>

        <div className="flex items-center gap-2">
          <Tabs value={period} onValueChange={(v) => setPeriod(v as ActivityPeriod)}>
            <TabsList>
              <TabsTrigger value="AllTime">Tout</TabsTrigger>
              <TabsTrigger value="Last30Days">30 jours</TabsTrigger>
              <TabsTrigger value="Last7Days">7 jours</TabsTrigger>
            </TabsList>
          </Tabs>

          <Button variant="destructive" size="sm" onClick={() => setClearModalOpen(true)}>
            <Trash2 className="h-3.5 w-3.5" />
            Effacer
          </Button>
        </div>
      </div>

      {/* Summary KPI cards */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
        <div className="card-oled rounded-2xl p-5">
          <div className="flex items-center gap-2 text-xs text-white/45">
            <Flame className="h-4 w-4 text-[#33F3FF]" />
            Écoutes totales
          </div>
          <p className="mt-2 text-3xl font-bold text-white tabular-nums">
            {stats?.total_plays ?? 0}
          </p>
          <p className="mt-1 text-xs text-white/40 tabular-nums">
            {stats?.plays_last_30_days ?? 0} ces 30 derniers jours
          </p>
        </div>

        <div className="card-oled rounded-2xl p-5">
          <div className="flex items-center gap-2 text-xs text-white/45">
            <Clock className="h-4 w-4 text-[#FFD432]" />
            Temps d&apos;écoute
          </div>
          <p className="mt-2 text-3xl font-bold text-white tabular-nums">
            {formatHours(stats?.total_secs ?? 0)}
          </p>
          <p className="mt-1 text-xs text-white/40 tabular-nums">
            {formatHours(stats?.secs_last_30_days ?? 0)} sur 30 jours
          </p>
        </div>

        <div className="card-oled rounded-2xl p-5">
          <div className="flex items-center gap-2 text-xs text-white/45">
            <Users className="h-4 w-4 text-[#30D158]" />
            Artistes distincts
          </div>
          <p className="mt-2 text-3xl font-bold text-white tabular-nums">
            {stats?.unique_artists ?? 0}
          </p>
          <p className="mt-1 text-xs text-white/40">Sur la période sélectionnée</p>
        </div>
      </div>

      {/* Last.fm card */}
      <section className="card-oled flex flex-wrap items-center justify-between gap-4 rounded-2xl p-5">
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <h2 className="text-sm font-semibold text-white">Scrobbling Last.fm</h2>
            <Badge variant={lastfm.connected ? "bitperfect" : "subtle"}>
              {lastfm.connected ? "Connecté" : "Déconnecté"}
            </Badge>
          </div>
          <p className="text-xs text-white/50">
            {lastfm.connected
              ? `Compte actif : ${lastfm.username ?? "Last.fm"} (stocké dans le Gestionnaire d'identification Windows)`
              : "Enregistrez automatiquement vos écoutes (> 30 s, à 50 % du titre) sur Last.fm."}
          </p>
        </div>

        {lastfm.connected ? (
          <Button variant="secondary" size="sm" onClick={() => void handleDisconnectLastFm()}>
            Déconnecter
          </Button>
        ) : (
          <Button size="sm" onClick={() => setLastfmModalOpen(true)}>
            Connecter Last.fm
          </Button>
        )}
      </section>

      {/* Top Tracks & Top Artists */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        <section className="card-oled rounded-2xl p-5 space-y-3">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-white/50">
            Titres les plus écoutés
          </h2>
          {stats && stats.top_tracks.length > 0 ? (
            <div className="space-y-2">
              {stats.top_tracks.map((item, idx) => (
                <div
                  key={`${item.track.id}-${idx}`}
                  className="flex items-center gap-3 rounded-xl px-2 py-1.5 hover:bg-white/[0.05] transition-colors"
                >
                  <span className="w-5 text-center text-xs text-white/40 tabular-nums">
                    {idx + 1}
                  </span>
                  <CoverImage
                    coverId={item.track.cover_id}
                    alt={item.track.title}
                    size={80}
                    className="h-10 w-10"
                    rounded="md"
                  />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium text-white">{item.track.title}</p>
                    <p className="truncate text-xs text-white/45">{item.track.artist_name}</p>
                  </div>
                  <span className="text-xs text-white/50 tabular-nums">
                    {item.play_count}× • {formatHours(item.total_secs)}
                  </span>
                  <button
                    type="button"
                    aria-label={`Lire ${item.track.title}`}
                    onClick={() => void commands.playTracks([item.track], 0)}
                    className="flex h-7 w-7 items-center justify-center rounded-lg text-white/50 hover:bg-white/10 hover:text-white cursor-pointer"
                  >
                    <Play className="h-3.5 w-3.5 fill-current" />
                  </button>
                </div>
              ))}
            </div>
          ) : (
            <p className="py-8 text-center text-xs text-white/35">Aucune écoute enregistrée.</p>
          )}
        </section>

        <section className="card-oled rounded-2xl p-5 space-y-3">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-white/50">
            Artistes les plus écoutés
          </h2>
          {stats && stats.top_artists.length > 0 ? (
            <div className="space-y-2">
              {stats.top_artists.map((art, idx) => (
                <button
                  key={`${art.artist_name}-${idx}`}
                  type="button"
                  onClick={() =>
                    art.artist_id && navigate({ name: "artist", artistId: art.artist_id })
                  }
                  className="flex w-full items-center gap-3 rounded-xl px-2 py-2 text-left hover:bg-white/[0.05] transition-colors cursor-pointer"
                >
                  <span className="w-5 text-center text-xs text-white/40 tabular-nums">
                    {idx + 1}
                  </span>
                  <Disc3 className="h-4 w-4 text-white/35 shrink-0" />
                  <span className="truncate flex-1 text-sm font-medium text-white">
                    {art.artist_name}
                  </span>
                  <span className="text-xs text-white/50 tabular-nums">
                    {art.play_count} écoutes • {formatHours(art.total_secs)}
                  </span>
                </button>
              ))}
            </div>
          ) : (
            <p className="py-8 text-center text-xs text-white/35">Aucun artiste enregistré.</p>
          )}
        </section>
      </div>

      {/* Last.fm Login Modal */}
      <Dialog open={lastfmModalOpen} onOpenChange={setLastfmModalOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Connexion Last.fm</DialogTitle>
            <DialogDescription>
              Renseignez vos identifiants API Last.fm (clé + secret) et votre compte.
            </DialogDescription>
          </DialogHeader>
          <form onSubmit={handleConnectLastFm} className="space-y-3">
            <Input
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="API Key"
              required
            />
            <Input
              value={apiSecret}
              onChange={(e) => setApiSecret(e.target.value)}
              placeholder="Shared Secret"
              type="password"
              required
            />
            <Input
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="Nom d'utilisateur Last.fm"
              required
            />
            <Input
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="Mot de passe"
              type="password"
              required
            />
            <div className="flex justify-end gap-2 pt-2">
              <Button type="button" variant="ghost" onClick={() => setLastfmModalOpen(false)}>
                Annuler
              </Button>
              <Button type="submit" disabled={submitting}>
                {submitting ? "Connexion…" : "Connecter"}
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* Clear History Confirmation */}
      <Dialog open={clearModalOpen} onOpenChange={setClearModalOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Effacer l&apos;historique local ?</DialogTitle>
            <DialogDescription>
              Cette action supprime définitivement vos compteurs d&apos;écoutes SQLite locaux.
            </DialogDescription>
          </DialogHeader>
          <div className="flex justify-end gap-2 pt-2">
            <Button variant="ghost" onClick={() => setClearModalOpen(false)}>
              Annuler
            </Button>
            <Button variant="destructive" onClick={() => void handleClearHistory()}>
              Confirmer la suppression
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
