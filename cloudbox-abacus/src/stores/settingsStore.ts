import { create } from "zustand";
import { api } from "../lib/api";
import type { Config } from "../lib/types";
import { useSyncStore } from "./syncStore";

export const defaultConfig: Config = {
  server: { host: "u678016.your-storagebox.de", username: "u678016", ssh_port: 23, protocol: "sftp" },
  mount: { enabled: false, mount_point: "", remote_path: "", cache_mode: "full", cache_max_size: "5G", transfers: 4 },
  bandwidth: { upload_limit_kbps: 0, download_limit_kbps: 0 },
  ui: { autostart: false, minimize_to_tray: true, language: "ru" },
  profiles: [],
};

interface SettingsState {
  config: Config;
  loaded: boolean;
  passwordStored: boolean;
  load: () => Promise<void>;
  save: (config: Config) => Promise<void>;
  setPasswordStored: (v: boolean) => void;
}

export const useSettingsStore = create<SettingsState>((set) => ({
  config: defaultConfig,
  loaded: false,
  passwordStored: false,
  load: async () => {
    const config = await api.getConfig();
    const passwordStored = await api.checkPasswordStored().catch(() => false);
    set({ config, loaded: true, passwordStored });
    useSyncStore.getState().setProfiles(config.profiles);
  },
  save: async (config) => {
    await api.saveConfig(config);
    set({ config });
    useSyncStore.getState().setProfiles(config.profiles);
  },
  setPasswordStored: (passwordStored) => set({ passwordStored }),
}));
