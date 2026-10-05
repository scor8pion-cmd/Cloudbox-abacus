// Mirrors the Rust structs serialised by Tauri commands.

export interface ServerConfig {
  host: string;
  username: string;
  ssh_port: number;
  protocol: string;
}

export interface MountConfig {
  enabled: boolean;
  mount_point: string;
  remote_path: string;
  cache_mode: string;
  cache_max_size: string;
  transfers: number;
}

export type SyncMode = "one-way-up";
export type SyncSchedule = "manual" | "realtime" | "interval";

export interface SyncProfile {
  id: string;
  name: string;
  local_path: string;
  remote_path: string;
  mode: SyncMode;
  schedule: SyncSchedule;
  interval_minutes: number;
  enabled: boolean;
  exclude: string[];
}

export interface BandwidthConfig {
  upload_limit_kbps: number;
  download_limit_kbps: number;
}

export interface UiConfig {
  autostart: boolean;
  minimize_to_tray: boolean;
  language: string;
}

export interface Config {
  server: ServerConfig;
  mount: MountConfig;
  bandwidth: BandwidthConfig;
  ui: UiConfig;
  profiles: SyncProfile[];
}

export type ConnStatus = "connected" | "disconnected" | "connecting";

export interface ConnectionStatus {
  status: ConnStatus;
  latency_ms: number;
  host: string;
  username: string;
}

export interface MountStatus {
  status: "mounted" | "unmounted" | "error";
  mount_point: string | null;
  rclone_available: boolean;
}

export interface FileEntry {
  name: string;
  path: string;
  size: number;
  is_dir: boolean;
  modified: number;
}

export interface StorageInfo {
  total_bytes: number;
  used_bytes: number;
  available_bytes: number;
}

export interface Operation {
  id: number;
  op_type: string;
  local_path: string;
  remote_path: string;
  status: string;
  retry_count: number;
  error_msg: string | null;
  created_at: string;
  updated_at: string;
}

export type ProfileState = "idle" | "syncing" | "completed" | "error" | "cancelled";

export interface ProfileStatus {
  profile_id: string;
  status: ProfileState;
  progress: number;
  current_file: string;
  files_synced: number;
  bytes_transferred: number;
  last_sync: string | null;
  last_run_epoch: number;
  last_error: string | null;
}

// Event payloads
export interface SyncProgressEvent {
  profile_id: string;
  file: string;
  progress: number;
  speed_bytes_s: number;
  uploaded: number;
  total: number;
}
export interface SyncCompletedEvent {
  profile_id: string;
  files_synced: number;
  bytes_transferred: number;
}
export interface SyncErrorEvent {
  profile_id: string;
  message: string;
}
export interface ConnectionChangedEvent {
  status: ConnStatus;
  latency_ms: number;
}
export interface MountChangedEvent {
  status: "mounted" | "unmounted" | "error";
  mount_point: string | null;
  message?: string;
}
export interface SpeedUpdateEvent {
  upload_bytes_s: number;
  download_bytes_s: number;
}
