import { create } from "zustand";
import type { Operation, ProfileStatus, SyncProfile } from "../lib/types";

export interface LiveProgress {
  file: string;
  progress: number;
  speed: number;
  uploaded: number;
  total: number;
}

interface SyncState {
  profiles: SyncProfile[];
  operations: Operation[];
  activeSync: Set<string>;
  statuses: Record<string, ProfileStatus>;
  progress: Record<string, LiveProgress>;
  lastError: string | null;
  setProfiles: (profiles: SyncProfile[]) => void;
  setOperations: (ops: Operation[]) => void;
  setStatuses: (list: ProfileStatus[]) => void;
  markActive: (id: string, active: boolean) => void;
  setProgress: (id: string, p: LiveProgress) => void;
  setError: (msg: string | null) => void;
}

export const useSyncStore = create<SyncState>((set) => ({
  profiles: [],
  operations: [],
  activeSync: new Set(),
  statuses: {},
  progress: {},
  lastError: null,
  setProfiles: (profiles) => set({ profiles }),
  setOperations: (operations) => set({ operations }),
  setStatuses: (list) =>
    set(() => {
      const statuses: Record<string, ProfileStatus> = {};
      const active = new Set<string>();
      for (const s of list) {
        statuses[s.profile_id] = s;
        if (s.status === "syncing") active.add(s.profile_id);
      }
      return { statuses, activeSync: active };
    }),
  markActive: (id, active) =>
    set((s) => {
      const next = new Set(s.activeSync);
      if (active) next.add(id);
      else next.delete(id);
      return { activeSync: next };
    }),
  setProgress: (id, p) => set((s) => ({ progress: { ...s.progress, [id]: p } })),
  setError: (lastError) => set({ lastError }),
}));
