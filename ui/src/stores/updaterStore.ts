import { toast } from "sonner";
import { create } from "zustand";
import { type AppUpdateInfo, commands } from "@/bindings";

interface UpdaterStoreState {
  info: AppUpdateInfo | null;
  checking: boolean;
  downloading: boolean;
  downloadPercent: number;
  error: string | null;
  loadStatus: () => Promise<AppUpdateInfo | null>;
  checkForUpdates: (options?: { silent?: boolean }) => Promise<AppUpdateInfo | null>;
  downloadUpdate: () => Promise<AppUpdateInfo | null>;
  applyAndRestart: () => Promise<void>;
  setProgress: (percent: number, stage: string) => void;
}

export const useUpdaterStore = create<UpdaterStoreState>((set, get) => ({
  info: null,
  checking: false,
  downloading: false,
  downloadPercent: 0,
  error: null,

  loadStatus: async () => {
    try {
      const res = await commands.getAppUpdateStatus();
      if (res.status === "ok") {
        set({ info: res.data, error: null });
        return res.data;
      }
      return null;
    } catch {
      return null;
    }
  },

  checkForUpdates: async (options) => {
    const silent = options?.silent ?? false;
    if (get().checking) return get().info;
    set({ checking: true, error: null });
    try {
      const res = await commands.checkForUpdates();
      if (res.status === "ok") {
        const info = res.data;
        set({ info, checking: false, error: null });
        if (!silent) {
          if (!info.is_installed) {
            toast.info("Mode portable / dev : installez TideNative via le Setup pour activer les mises à jour automatiques.");
          } else if (info.pending_restart && info.available_version) {
            toast.success(`TideNative v${info.available_version} est prête à être installée.`, {
              action: {
                label: "Redémarrer",
                onClick: () => void get().applyAndRestart(),
              },
            });
          } else if (info.update_available && info.available_version) {
            toast.info(`Nouvelle version TideNative v${info.available_version} disponible`, {
              action: {
                label: "Télécharger",
                onClick: () => void get().downloadUpdate(),
              },
            });
          } else {
            toast.success(`TideNative v${info.current_version} est déjà à jour.`);
          }
        }
        return info;
      }
      set({ checking: false, error: res.error });
      if (!silent) toast.error(res.error);
      return null;
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Erreur de vérification des mises à jour";
      set({ checking: false, error: msg });
      if (!silent) toast.error(msg);
      return null;
    }
  },

  downloadUpdate: async () => {
    if (get().downloading) return get().info;
    set({ downloading: true, downloadPercent: 0, error: null });
    try {
      const res = await commands.downloadUpdate();
      if (res.status === "ok") {
        const info = res.data;
        set({ info, downloading: false, downloadPercent: info.pending_restart ? 100 : 0 });
        if (info.pending_restart && info.available_version) {
          toast.success(`TideNative v${info.available_version} téléchargée`, {
            description: "Redémarrez pour appliquer la mise à jour.",
            duration: 12000,
            action: {
              label: "Redémarrer",
              onClick: () => void get().applyAndRestart(),
            },
          });
        }
        return info;
      }
      set({ downloading: false, error: res.error });
      toast.error(res.error);
      return null;
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Échec du téléchargement";
      set({ downloading: false, error: msg });
      toast.error(msg);
      return null;
    }
  },

  applyAndRestart: async () => {
    try {
      const res = await commands.applyUpdateAndRestart();
      if (res.status === "error") {
        toast.error(res.error);
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Impossible d'appliquer la mise à jour");
    }
  },

  setProgress: (percent, stage) => {
    if (stage === "downloading") {
      set({ downloading: true, downloadPercent: Math.max(0, Math.min(100, percent)) });
    } else if (stage === "ready") {
      set({ downloading: false, downloadPercent: 100 });
    } else if (stage === "error") {
      set({ downloading: false });
    }
  },
}));
