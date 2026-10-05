import { useEffect } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api } from "../lib/api";
import { isTauri } from "../lib/utils";
import type {
  ConnectionChangedEvent,
  MountChangedEvent,
  SpeedUpdateEvent,
  SyncCompletedEvent,
  SyncErrorEvent,
  SyncProgressEvent,
} from "../lib/types";
import { useConnectionStore } from "../stores/connectionStore";
import { useSyncStore } from "../stores/syncStore";

/** Refreshes storage quota, operations and sync statuses from the backend. */
export async function refreshAll() {
  const conn = useConnectionStore.getState();
  const sync = useSyncStore.getState();
  const [status, mount, ops, statuses] = await Promise.allSettled([
    api.getConnectionStatus(),
    api.getMountStatus(),
    api.getOperations(),
    api.getSyncStatus(),
  ]);
  if (status.status === "fulfilled") conn.setStatus(status.value.status, status.value.latency_ms);
  if (mount.status === "fulfilled") conn.setMount(mount.value);
  if (ops.status === "fulfilled") sync.setOperations(ops.value);
  if (statuses.status === "fulfilled") sync.setStatuses(statuses.value);
  if (status.status === "fulfilled" && status.value.status === "connected") {
    api.getStorageInfo().then((s) => conn.setStorage(s.used_bytes, s.total_bytes)).catch(() => {});
  }
}

/** Subscribes to all backend events and keeps the Zustand stores in sync. */
export function useTauriEvents() {
  useEffect(() => {
    if (!isTauri) return;
    const unlisteners: Promise<UnlistenFn>[] = [];
    const conn = useConnectionStore.getState;
    const sync = useSyncStore.getState;

    unlisteners.push(
      listen<SyncProgressEvent>("sync-progress", ({ payload }) => {
        sync().markActive(payload.profile_id, payload.progress < 1);
        sync().setProgress(payload.profile_id, {
          file: payload.file,
          progress: payload.progress,
          speed: payload.speed_bytes_s,
          uploaded: payload.uploaded,
          total: payload.total,
        });
      }),
      listen<SyncCompletedEvent>("sync-completed", ({ payload }) => {
        sync().markActive(payload.profile_id, false);
        refreshAll();
      }),
      listen<SyncErrorEvent>("sync-error", ({ payload }) => {
        sync().setError(payload.message);
        refreshAll();
      }),
      listen<ConnectionChangedEvent>("connection-changed", ({ payload }) => {
        const prev = conn().status;
        conn().setStatus(payload.status, payload.latency_ms);
        if (payload.status === "connected" && prev !== "connected") refreshAll();
      }),
      listen<MountChangedEvent>("mount-changed", ({ payload }) => {
        conn().setMount({ status: payload.status, mount_point: payload.mount_point });
      }),
      listen<SpeedUpdateEvent>("speed-update", ({ payload }) => {
        conn().pushSpeed(payload.upload_bytes_s, payload.download_bytes_s);
      }),
    );

    refreshAll();
    const timer = setInterval(refreshAll, 5000);
    return () => {
      clearInterval(timer);
      unlisteners.forEach((p) => p.then((un) => un()));
    };
  }, []);
}
