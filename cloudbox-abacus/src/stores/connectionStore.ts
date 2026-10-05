import { create } from "zustand";
import type { ConnStatus, MountStatus } from "../lib/types";

export interface SpeedPoint {
  t: number;
  upload: number;
  download: number;
}

const HISTORY = 60;

interface ConnectionState {
  status: ConnStatus;
  latency_ms: number;
  upload_speed: number;
  download_speed: number;
  storage: { used: number; total: number };
  mount: MountStatus;
  speedHistory: SpeedPoint[];
  setStatus: (status: ConnStatus, latency_ms?: number) => void;
  pushSpeed: (upload: number, download: number) => void;
  setStorage: (used: number, total: number) => void;
  setMount: (mount: Partial<MountStatus>) => void;
}

const emptyHistory = (): SpeedPoint[] => {
  const now = Date.now();
  return Array.from({ length: HISTORY }, (_, i) => ({ t: now - (HISTORY - i) * 1000, upload: 0, download: 0 }));
};

export const useConnectionStore = create<ConnectionState>((set) => ({
  status: "disconnected",
  latency_ms: 0,
  upload_speed: 0,
  download_speed: 0,
  storage: { used: 0, total: 0 },
  mount: { status: "unmounted", mount_point: null, rclone_available: true },
  speedHistory: emptyHistory(),
  setStatus: (status, latency_ms) =>
    set((s) => ({ status, latency_ms: latency_ms ?? (status === "connected" ? s.latency_ms : 0) })),
  pushSpeed: (upload, download) =>
    set((s) => ({
      upload_speed: upload,
      download_speed: download,
      speedHistory: [...s.speedHistory.slice(-(HISTORY - 1)), { t: Date.now(), upload, download }],
    })),
  setStorage: (used, total) => set({ storage: { used, total } }),
  setMount: (mount) => set((s) => ({ mount: { ...s.mount, ...mount } })),
}));
