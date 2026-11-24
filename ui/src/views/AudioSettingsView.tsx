import { FolderOpen, LogOut, RefreshCw, ShieldCheck, Type } from "lucide-react";
import { toast } from "sonner";
import {
  type AppPreferences,
  type AudioQuality,
  type ReplayGainMode,
  type UiFontFamily,
  commands,
} from "@/bindings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { UI_FONT_STACKS, usePlaybackStore } from "@/stores/playbackStore";

const QUALITY_OPTIONS: { value: AudioQuality; label: string }[] = [
  { value: "HiResLossless", label: "MAX — Hi-Res FLAC (24-bit / 192 kHz)" },
  { value: "Lossless", label: "HIGH — FLAC CD (16-bit / 44.1 kHz)" },
  { value: "High", label: "AAC 320 kbps" },
  { value: "Low", label: "LOW — AAC 96 kbps" },
];

const REPLAYGAIN_OPTIONS: { value: ReplayGainMode; label: string }[] = [
  { value: "Off", label: "Désactivé (Bit-Perfect pur)" },
  { value: "Track", label: "Par piste" },
  { value: "Album", label: "Par album" },
];

const UI_FONT_OPTIONS: {
  value: UiFontFamily;
  label: string;
  subtitle: string;
}[] = [
  {
    value: "Default",
    label: "Inter (Actuelle)",
    subtitle: "Police par défaut précise et équilibrée",
  },
  {
    value: "SeasonMix",
    label: "season-mix",
    subtitle: "Season Mix — caractère éditorial & audiophile",
  },
  {
    value: "DmSans",
    label: "DM Sans",
    subtitle: "Géométrique moderne à haute lisibilité",
  },
  {
    value: "PlusJakartaSans",
    label: "Plus Jakarta Sans",
    subtitle: "Contemporaine, fluide et chaleureuse",
  },
];

export function AudioSettingsView() {
  const preferences = usePlaybackStore((s) => s.preferences);
  const setPreferences = usePlaybackStore((s) => s.setPreferences);
  const devices = usePlaybackStore((s) => s.devices);
  const activeFormat = usePlaybackStore((s) => s.activeFormat);
  const authenticated = usePlaybackStore((s) => s.authenticated);
  const userId = usePlaybackStore((s) => s.userId);
  const countryCode = usePlaybackStore((s) => s.countryCode);
  const currentFont: UiFontFamily = preferences.ui_font ?? "Default";

  const patchPrefs = async (patch: Partial<AppPreferences>) => {
    const next: AppPreferences = { ...preferences, ...patch };
    setPreferences(next);
    const res = await commands.updatePreferences(next);
    if (res.status === "error") {
      toast.error(res.error);
    }
  };

  const handleRefreshDevices = async () => {
    const res = await commands.refreshDevices();
    if (res.status === "ok") {
      toast.success(`${res.data.length} périphérique(s) WASAPI détecté(s)`);
    } else {
      toast.error(res.error);
    }
  };

  const handleLogout = async () => {
    const res = await commands.logout();
    if (res.status === "ok") {
      toast.info("Déconnecté de TIDAL");
    } else {
      toast.error(res.error);
    }
  };

  return (
    <div className="mx-auto w-full max-w-3xl space-y-6 pb-10">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight text-white">Paramètres Audio</h1>
        <p className="text-xs text-white/45 mt-0.5">
          Pipeline WASAPI Windows Bit-Perfect, qualité Hi-Res et préférences bureau
        </p>
      </div>

      {/* Live Hardware Diagnostic */}
      <section className="card-oled rounded-2xl p-5 space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <ShieldCheck className="h-4 w-4 text-[#30D158]" />
            <h2 className="text-sm font-semibold text-white">Diagnostic du Pipeline Actif</h2>
          </div>
          {activeFormat?.is_bit_perfect && <Badge variant="bitperfect">Bit-Perfect Actif</Badge>}
        </div>
        {activeFormat ? (
          <div className="grid grid-cols-2 gap-3 text-xs sm:grid-cols-4">
            <div className="rounded-xl bg-white/[0.03] p-3">
              <p className="text-white/40">Codec</p>
              <p className="mt-1 font-semibold text-white">{activeFormat.codec}</p>
            </div>
            <div className="rounded-xl bg-white/[0.03] p-3">
              <p className="text-white/40">Source PCM</p>
              <p className="mt-1 font-semibold text-white tabular-nums">
                {activeFormat.source_pcm.bits_per_sample}-bit /{" "}
                {(activeFormat.source_pcm.sample_rate / 1000).toFixed(1)} kHz
              </p>
            </div>
            <div className="rounded-xl bg-white/[0.03] p-3">
              <p className="text-white/40">Conteneur DAC</p>
              <p className="mt-1 font-semibold text-white tabular-nums">
                {activeFormat.dac_container_bits}-bit ({activeFormat.dac_valid_bits}b)
              </p>
            </div>
            <div className="rounded-xl bg-white/[0.03] p-3">
              <p className="text-white/40">Mode WASAPI</p>
              <p className="mt-1 font-semibold text-white">
                {activeFormat.is_exclusive ? "Exclusif" : "Partagé"}
              </p>
            </div>
          </div>
        ) : (
          <p className="text-xs text-white/45">
            Aucun flux actif. Lancez un titre pour inspecter la négociation matérielle WASAPI.
          </p>
        )}
      </section>

      {/* Output Device & Quality */}
      <section className="card-oled rounded-2xl p-5 space-y-5">
        <h2 className="text-sm font-semibold uppercase tracking-wider text-white/50">
          Sortie Matérielle & Qualité
        </h2>

        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <label className="text-sm font-medium text-white">Périphérique de sortie DAC / WASAPI</label>
            <Button variant="ghost" size="sm" onClick={() => void handleRefreshDevices()}>
              <RefreshCw className="h-3.5 w-3.5" />
              Actualiser
            </Button>
          </div>
          <Select
            value={preferences.device_id ?? "__default__"}
            onValueChange={(val) =>
              void patchPrefs({ device_id: val === "__default__" ? null : val })
            }
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="__default__">Périphérique Windows par défaut</SelectItem>
              {devices.map((dev) => (
                <SelectItem key={dev.id} value={dev.id}>
                  {dev.name} {dev.is_default ? "(Défaut)" : ""}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="space-y-2">
          <label className="text-sm font-medium text-white">Qualité de streaming TIDAL</label>
          <Select
            value={preferences.audio_quality}
            onValueChange={(val) => void patchPrefs({ audio_quality: val as AudioQuality })}
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {QUALITY_OPTIONS.map((opt) => (
                <SelectItem key={opt.value} value={opt.value}>
                  {opt.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <Separator />

        <SettingRow
          title="Mode exclusif WASAPI"
          description="Accès matériel direct au DAC (contourne le mixeur Windows, horloge native)."
          checked={preferences.exclusive_mode}
          onCheckedChange={(checked) => void patchPrefs({ exclusive_mode: checked })}
        />

        <SettingRow
          title="Repli automatique en mode partagé"
          description="Bascule sans erreur en WASAPI Partagé si le DAC est occupé par une autre app."
          checked={preferences.fallback_to_shared}
          onCheckedChange={(checked) => void patchPrefs({ fallback_to_shared: checked })}
        />

        <SettingRow
          title="Verrouillage Bit-Perfect (Volume fixe 100 %)"
          description="Désactive toute atténuation logicielle pour garantir l'intégrité binaire."
          checked={preferences.bit_perfect_lock}
          onCheckedChange={(checked) => void patchPrefs({ bit_perfect_lock: checked })}
        />

        <div className="space-y-2 pt-1">
          <label className="text-sm font-medium text-white">Normalisation ReplayGain</label>
          <Select
            value={preferences.replay_gain_mode}
            onValueChange={(val) => {
              const mode = val as ReplayGainMode;
              void patchPrefs({
                replay_gain_mode: mode,
                normalize_volume: mode !== "Off",
              });
            }}
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {REPLAYGAIN_OPTIONS.map((opt) => (
                <SelectItem key={opt.value} value={opt.value}>
                  {opt.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </section>

      {/* Appearance & Typography */}
      <section className="card-oled rounded-2xl p-5 space-y-4">
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <Type className="h-4 w-4 text-[#33F3FF]" />
            <h2 className="text-sm font-semibold uppercase tracking-wider text-white/50">
              Apparence & Typographie
            </h2>
          </div>
          <div className="w-56">
            <Select
              value={currentFont}
              onValueChange={(val) => void patchPrefs({ ui_font: val as UiFontFamily })}
            >
              <SelectTrigger aria-label="Police de l'application">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {UI_FONT_OPTIONS.map((opt) => (
                  <SelectItem
                    key={opt.value}
                    value={opt.value}
                    style={{ fontFamily: UI_FONT_STACKS[opt.value] }}
                  >
                    {opt.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          {UI_FONT_OPTIONS.map((opt) => {
            const isSelected = currentFont === opt.value;
            return (
              <button
                key={opt.value}
                type="button"
                onClick={() => void patchPrefs({ ui_font: opt.value })}
                style={{ fontFamily: UI_FONT_STACKS[opt.value] }}
                className={`flex flex-col justify-between rounded-xl border p-3.5 text-left transition-all duration-200 cursor-pointer ${
                  isSelected
                    ? "border-[#33F3FF]/50 bg-[#33F3FF]/10 text-white"
                    : "border-white/[0.08] bg-white/[0.02] hover:border-white/20 hover:bg-white/[0.04]"
                }`}
              >
                <div className="flex items-center justify-between gap-2">
                  <span
                    className={`text-sm font-semibold ${
                      isSelected ? "text-[#33F3FF]" : "text-white"
                    }`}
                  >
                    {opt.label}
                  </span>
                  <span className="text-xs text-white/40">Aa Bb 123</span>
                </div>
                <p className="mt-1 text-xs text-white/50">{opt.subtitle}</p>
              </button>
            );
          })}
        </div>
      </section>

      {/* Playback & Desktop Behavior */}
      <section className="card-oled rounded-2xl p-5 space-y-4">
        <h2 className="text-sm font-semibold uppercase tracking-wider text-white/50">
          Lecture & Comportement Bureau
        </h2>

        <SettingRow
          title="Enchaînement Gapless"
          description="Précharge et décode la piste suivante sans coupure entre deux morceaux."
          checked={preferences.gapless_playback ?? true}
          onCheckedChange={(checked) => void patchPrefs({ gapless_playback: checked })}
        />

        <SettingRow
          title="Radio automatique en fin de file"
          description="Poursuit l'écoute avec des titres similaires lorsque la file se termine."
          checked={preferences.autoplay_radio ?? true}
          onCheckedChange={(checked) => void patchPrefs({ autoplay_radio: checked })}
        />

        <SettingRow
          title="Réduire dans la zone de notification à la fermeture"
          description="Maintient la lecture active en arrière-plan depuis l'icône Windows Tray."
          checked={preferences.minimize_to_tray ?? false}
          onCheckedChange={(checked) => void patchPrefs({ minimize_to_tray: checked })}
        />

        <SettingRow
          title="Autoriser les contenus explicites (E)"
          description="Affiche et lit les pistes marquées explicites."
          checked={preferences.allow_explicit ?? true}
          onCheckedChange={(checked) => void patchPrefs({ allow_explicit: checked })}
        />

        <SettingRow
          title="Autoriser les contenus générés par IA"
          description="Filtre par défaut les pistes identifiées comme générées par IA."
          checked={preferences.allow_ai_content ?? false}
          onCheckedChange={(checked) => void patchPrefs({ allow_ai_content: checked })}
        />
      </section>

      {/* Account & Data */}
      <section className="card-oled flex flex-wrap items-center justify-between gap-4 rounded-2xl p-5">
        <div>
          <h2 className="text-sm font-semibold text-white">Compte & Données Locales</h2>
          <p className="text-xs text-white/45 mt-0.5 tabular-nums">
            {authenticated
              ? `Session TIDAL active (ID ${userId ?? "—"} • ${countryCode ?? "—"})`
              : "Aucune session TIDAL active"}
          </p>
        </div>

        <div className="flex items-center gap-2">
          <Button variant="secondary" size="sm" onClick={() => void commands.openDataFolder()}>
            <FolderOpen className="h-3.5 w-3.5" />
            Dossier de données
          </Button>
          {authenticated && (
            <Button variant="destructive" size="sm" onClick={() => void handleLogout()}>
              <LogOut className="h-3.5 w-3.5" />
              Déconnexion
            </Button>
          )}
        </div>
      </section>
    </div>
  );
}

interface SettingRowProps {
  title: string;
  description: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}

function SettingRow({ title, description, checked, onCheckedChange }: SettingRowProps) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="space-y-0.5">
        <p className="text-sm font-medium text-white">{title}</p>
        <p className="text-xs text-white/45">{description}</p>
      </div>
      <Switch aria-label={title} checked={checked} onCheckedChange={onCheckedChange} />
    </div>
  );
}
