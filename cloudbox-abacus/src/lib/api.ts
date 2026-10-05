// Typed wrappers around Tauri IPC commands.
import { invoke } from "@tauri-apps/api/core";
import type {
  Config,
  ConnectionStatus,
  FileEntry,
  MountStatus,
  Operation,
  ProfileStatus,
  StorageInfo,
} from "./types";

export const api = {
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  connect: (config: Config, password: string) => invoke<void>("connect", { config, password }),
  disconnect: () => invoke<void>("disconnect"),
  testConnection: (host: string, port: number, user: string, password: string) =>
    invoke<string>("test_connection", { host, port, user, password }),
  getConnectionStatus: () => invoke<ConnectionStatus>("get_connection_status"),
  getRemoteHome: () => invoke<string>("get_remote_home"),
  listRemote: (path: string) => invoke<FileEntry[]>("list_remote", { path }),
  uploadFiles: (localPaths: string[], remotePath: string) =>
    invoke<void>("upload_files", { localPaths, remotePath }),
  downloadFile: (remotePath: string, localPath: string) =>
    invoke<void>("download_file", { remotePath, localPath }),
  deleteRemote: (path: string) => invoke<void>("delete_remote", { path }),
  createRemoteDir: (path: string) => invoke<void>("create_remote_dir", { path }),
  mountDrive: () => invoke<string>("mount_drive"),
  unmountDrive: () => invoke<void>("unmount_drive"),
  getMountStatus: () => invoke<MountStatus>("get_mount_status"),
  startSync: (profileId: string) => invoke<void>("start_sync", { profileId }),
  stopSync: (profileId: string) => invoke<void>("stop_sync", { profileId }),
  getSyncStatus: () => invoke<ProfileStatus[]>("get_sync_status"),
  getOperations: () => invoke<Operation[]>("get_operations"),
  getStorageInfo: () => invoke<StorageInfo>("get_storage_info"),
  storePassword: (password: string) => invoke<void>("store_password", { password }),
  deletePassword: () => invoke<void>("delete_password"),
  checkPasswordStored: () => invoke<boolean>("check_password_stored"),
};
