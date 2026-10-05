//! CloudBox — Tauri application setup and IPC commands.

pub mod config;
pub mod database;
pub mod file_watcher;
pub mod mount;
pub mod secrets;
pub mod sftp;
pub mod sync_engine;

use config::Config;
use database::{Database, Operation};
use file_watcher::FileWatcher;
use mount::MountManager;
use serde::Serialize;
use serde_json::json;
use sftp::{FileEntry, SftpClient, StorageInfo};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use sync_engine::{scan_local, SyncEngine};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_notification::NotificationExt;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionStatus {
    pub status: String,
    pub latency_ms: u64,
    pub host: String,
    pub username: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MountStatus {
    pub status: String,
    pub mount_point: Option<String>,
    pub rclone_available: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProfileStatus {
    pub profile_id: String,
    /// "idle" | "syncing" | "completed" | "error" | "cancelled"
    pub status: String,
    pub progress: f32,
    pub current_file: String,
    pub files_synced: u32,
    pub bytes_transferred: u64,
    pub last_sync: Option<String>,
    pub last_run_epoch: u64,
    pub last_error: Option<String>,
}

pub struct AppState {
    pub config: Config,
    /// Session password kept in memory only.
    pub password: Option<String>,
    pub sftp: Option<Arc<Mutex<SftpClient>>>,
    pub conn_status: String,
    pub latency_ms: u64,
    pub mount: Arc<Mutex<MountManager>>,
    pub db: Arc<Mutex<Database>>,
    pub sync_flags: HashMap<String, Arc<AtomicBool>>,
    pub profile_status: Arc<Mutex<HashMap<String, ProfileStatus>>>,
    pub watcher: Option<FileWatcher>,
}

type Shared<'a> = State<'a, Mutex<AppState>>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn aerr(e: anyhow::Error) -> String {
    format!("{e:#}")
}

async fn blocking<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

fn resolve_password(state: &AppState, explicit: &str) -> Result<String, String> {
    if !explicit.is_empty() {
        return Ok(explicit.to_string());
    }
    if let Some(p) = &state.password {
        return Ok(p.clone());
    }
    secrets::get_password(secrets::SERVICE, &state.config.server.username)
        .map_err(|_| "Пароль не задан. Укажите его в настройках.".to_string())
}

fn current_sftp(state: &Shared<'_>) -> Result<Arc<Mutex<SftpClient>>, String> {
    state.lock().map_err(err)?.sftp.clone().ok_or_else(|| "Нет подключения к серверу".to_string())
}

fn emit_connection(app: &AppHandle, status: &str, latency_ms: u64) {
    app.emit("connection-changed", json!({ "status": status, "latency_ms": latency_ms })).ok();
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    app.notification().builder().title(title).body(body).show().ok();
}

/// (Re)starts the file watcher for realtime profiles.
fn restart_watcher(app: &AppHandle) {
    let state = app.state::<Mutex<AppState>>();
    let Ok(mut st) = state.lock() else { return };
    st.watcher = None;
    if st.sftp.is_none() {
        return;
    }
    let handle = app.clone();
    match FileWatcher::start(st.config.profiles.clone(), st.db.clone(), move |id| {
        spawn_sync(&handle, &id).ok();
    }) {
        Ok(w) => st.watcher = w,
        Err(e) => eprintln!("file watcher error: {e:#}"),
    }
}

/// Starts a background sync for one profile on a dedicated SFTP connection.
fn spawn_sync(app: &AppHandle, profile_id: &str) -> Result<(), String> {
    let state = app.state::<Mutex<AppState>>();
    let (profile, server, password, db, statuses, flag, bandwidth) = {
        let mut st = state.lock().map_err(err)?;
        let profile = st.config.profile(profile_id).cloned().ok_or("Профиль не найден")?;
        if st.sync_flags.contains_key(profile_id) {
            return Ok(()); // already running
        }
        let password = resolve_password(&st, "")?;
        let flag = Arc::new(AtomicBool::new(false));
        st.sync_flags.insert(profile_id.to_string(), flag.clone());
        (profile, st.config.server.clone(), password, st.db.clone(), st.profile_status.clone(), flag, st.config.bandwidth.clone())
    };

    {
        let mut map = statuses.lock().map_err(err)?;
        let s = map.entry(profile.id.clone()).or_default();
        s.profile_id = profile.id.clone();
        s.status = "syncing".into();
        s.progress = 0.0;
        s.last_error = None;
    }

    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let emit_app = app.clone();
        let progress_statuses = statuses.clone();
        let emitter: sync_engine::Emitter = Arc::new(move |name, payload| {
            if name == "sync-progress" {
                if let (Some(id), Ok(mut map)) = (payload["profile_id"].as_str(), progress_statuses.lock()) {
                    if let Some(s) = map.get_mut(id) {
                        s.progress = payload["progress"].as_f64().unwrap_or(0.0) as f32;
                        s.current_file = payload["file"].as_str().unwrap_or("").to_string();
                    }
                }
            }
            emit_app.emit(name, payload).ok();
        });

        let outcome = SftpClient::connect(&server.host, server.ssh_port, &server.username, &password).and_then(|client| {
            let engine = SyncEngine { db, sftp: Arc::new(Mutex::new(client)), emit: emitter.clone(), cancel: flag.clone(), bandwidth };
            engine.sync_profile(&profile)
        });

        let now = chrono::Local::now();
        if let Ok(mut map) = statuses.lock() {
            let s = map.entry(profile.id.clone()).or_default();
            s.last_sync = Some(now.to_rfc3339());
            s.last_run_epoch = now.timestamp() as u64;
            s.current_file.clear();
            match &outcome {
                Ok(r) => {
                    s.status = if r.cancelled { "cancelled".into() } else if r.files_failed > 0 { "error".into() } else { "completed".into() };
                    s.files_synced = r.files_synced;
                    s.bytes_transferred = r.bytes_transferred;
                    s.progress = 1.0;
                    if r.files_failed > 0 {
                        s.last_error = Some(format!("Ошибок: {}", r.files_failed));
                    }
                }
                Err(e) => {
                    s.status = "error".into();
                    s.last_error = Some(format!("{e:#}"));
                }
            }
        }

        match outcome {
            Ok(r) => {
                app.emit(
                    "sync-completed",
                    json!({ "profile_id": profile.id, "files_synced": r.files_synced, "bytes_transferred": r.bytes_transferred }),
                )
                .ok();
                if r.files_synced > 0 {
                    notify(&app, "CloudBox", &format!("«{}»: синхронизировано файлов — {}", profile.name, r.files_synced));
                }
            }
            Err(e) => {
                app.emit("sync-error", json!({ "profile_id": profile.id, "message": format!("{e:#}") })).ok();
                notify(&app, "CloudBox — ошибка синхронизации", &format!("«{}»: {e:#}", profile.name));
            }
        }
        if let Ok(mut st) = app.state::<Mutex<AppState>>().lock() {
            st.sync_flags.remove(&profile.id);
        }
    });
    Ok(())
}

fn sync_all(app: &AppHandle) {
    let ids: Vec<String> = match app.state::<Mutex<AppState>>().lock() {
        Ok(st) => st.config.profiles.iter().filter(|p| p.enabled).map(|p| p.id.clone()).collect(),
        Err(_) => return,
    };
    for id in ids {
        if let Err(e) = spawn_sync(app, &id) {
            app.emit("sync-error", json!({ "profile_id": id, "message": e })).ok();
        }
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn get_config(state: Shared<'_>) -> Result<Config, String> {
    Ok(state.lock().map_err(err)?.config.clone())
}

#[tauri::command]
async fn save_config(app: AppHandle, state: Shared<'_>, config: Config) -> Result<(), String> {
    config.save().map_err(aerr)?;
    let autostart = config.ui.autostart;
    state.lock().map_err(err)?.config = config;
    let launcher = app.autolaunch();
    let res = if autostart { launcher.enable() } else { launcher.disable() };
    if let Err(e) = res {
        eprintln!("autostart: {e}");
    }
    restart_watcher(&app);
    Ok(())
}

#[tauri::command]
async fn connect(app: AppHandle, state: Shared<'_>, config: Config, password: String) -> Result<(), String> {
    let password = {
        let mut st = state.lock().map_err(err)?;
        st.config = config.clone();
        st.conn_status = "connecting".into();
        resolve_password(&st, &password)?
    };
    config.save().map_err(aerr)?;
    emit_connection(&app, "connecting", 0);

    let s = config.server.clone();
    let pw = password.clone();
    let result = blocking(move || {
        let client = SftpClient::connect(&s.host, s.ssh_port, &s.username, &pw).map_err(aerr)?;
        let latency = client.ping().unwrap_or(0);
        Ok((client, latency))
    })
    .await;

    match result {
        Ok((client, latency)) => {
            {
                let mut st = state.lock().map_err(err)?;
                st.sftp = Some(Arc::new(Mutex::new(client)));
                st.password = Some(password);
                st.conn_status = "connected".into();
                st.latency_ms = latency;
            }
            emit_connection(&app, "connected", latency);
            restart_watcher(&app);
            Ok(())
        }
        Err(e) => {
            state.lock().map_err(err)?.conn_status = "disconnected".into();
            emit_connection(&app, "disconnected", 0);
            Err(e)
        }
    }
}

#[tauri::command]
async fn disconnect(app: AppHandle, state: Shared<'_>) -> Result<(), String> {
    {
        let mut st = state.lock().map_err(err)?;
        st.sftp = None;
        st.watcher = None;
        st.conn_status = "disconnected".into();
        st.latency_ms = 0;
        for flag in st.sync_flags.values() {
            flag.store(true, Ordering::Relaxed);
        }
    }
    emit_connection(&app, "disconnected", 0);
    Ok(())
}

#[tauri::command]
async fn test_connection(state: Shared<'_>, host: String, port: u16, user: String, password: String) -> Result<String, String> {
    let password = {
        let st = state.lock().map_err(err)?;
        if password.is_empty() {
            secrets::get_password(secrets::SERVICE, &user).ok().or_else(|| st.password.clone()).ok_or("Введите пароль")?
        } else {
            password
        }
    };
    blocking(move || {
        let client = SftpClient::connect(&host, port, &user, &password).map_err(aerr)?;
        let latency = client.ping().unwrap_or(0);
        let mut msg = format!("Подключение успешно. Задержка: {latency} мс.");
        if let Ok(info) = client.get_storage_info() {
            msg.push_str(&format!(
                " Занято {:.1} ГБ из {:.1} ГБ.",
                info.used_bytes as f64 / 1e9,
                info.total_bytes as f64 / 1e9
            ));
        }
        Ok(msg)
    })
    .await
}

#[tauri::command]
async fn get_connection_status(state: Shared<'_>) -> Result<ConnectionStatus, String> {
    let st = state.lock().map_err(err)?;
    Ok(ConnectionStatus {
        status: if st.sftp.is_some() { "connected".into() } else { st.conn_status.clone() },
        latency_ms: st.latency_ms,
        host: st.config.server.host.clone(),
        username: st.config.server.username.clone(),
    })
}

#[tauri::command]
async fn get_remote_home(state: Shared<'_>) -> Result<String, String> {
    let sftp = current_sftp(&state)?;
    blocking(move || sftp.lock().map_err(err)?.home().map_err(aerr)).await
}

#[tauri::command]
async fn list_remote(state: Shared<'_>, path: String) -> Result<Vec<FileEntry>, String> {
    let sftp = current_sftp(&state)?;
    blocking(move || {
        let client = sftp.lock().map_err(err)?;
        let path = if path.is_empty() { client.home().map_err(aerr)? } else { path };
        client.list_dir(&path).map_err(aerr)
    })
    .await
}

#[tauri::command]
async fn upload_files(app: AppHandle, state: Shared<'_>, local_paths: Vec<String>, remote_path: String) -> Result<(), String> {
    let sftp = current_sftp(&state)?;
    let db = state.lock().map_err(err)?.db.clone();
    blocking(move || {
        let client = sftp.lock().map_err(err)?;
        // Expand directories into (local file, remote path) pairs.
        let mut jobs: Vec<(PathBuf, String)> = Vec::new();
        for lp in &local_paths {
            let p = PathBuf::from(lp);
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let base = sftp::join_remote(&remote_path, &name);
            if p.is_dir() {
                for f in scan_local(&p, &[]).map_err(aerr)? {
                    jobs.push((f.path, sftp::join_remote(&base, &f.rel)));
                }
            } else {
                jobs.push((p, base));
            }
        }
        let total = jobs.len();
        let mut errors = Vec::new();
        for (i, (local, remote)) in jobs.into_iter().enumerate() {
            let local_s = local.to_string_lossy().into_owned();
            let op = db.lock().map_err(err)?.add_operation("upload", &local_s, &remote, "in_progress").map_err(aerr)?;
            app.emit(
                "sync-progress",
                json!({ "profile_id": "manual", "file": local_s, "progress": i as f32 / total.max(1) as f32, "speed_bytes_s": 0, "uploaded": i, "total": total }),
            )
            .ok();
            let res = client.upload_file(&local, &remote);
            let d = db.lock().map_err(err)?;
            match res {
                Ok(()) => d.update_operation(op, "done", None).map_err(aerr)?,
                Err(e) => {
                    d.update_operation(op, "error", Some(&format!("{e:#}"))).map_err(aerr)?;
                    errors.push(format!("{local_s}: {e:#}"));
                }
            }
        }
        app.emit("sync-completed", json!({ "profile_id": "manual", "files_synced": total - errors.len(), "bytes_transferred": 0 })).ok();
        if errors.is_empty() { Ok(()) } else { Err(errors.join("\n")) }
    })
    .await
}

#[tauri::command]
async fn download_file(state: Shared<'_>, remote_path: String, local_path: String) -> Result<(), String> {
    let sftp = current_sftp(&state)?;
    let db = state.lock().map_err(err)?.db.clone();
    blocking(move || {
        let op = db.lock().map_err(err)?.add_operation("download", &local_path, &remote_path, "in_progress").map_err(aerr)?;
        let res = sftp.lock().map_err(err)?.download_file(&remote_path, &PathBuf::from(&local_path));
        let d = db.lock().map_err(err)?;
        match res {
            Ok(()) => d.update_operation(op, "done", None).map_err(aerr),
            Err(e) => {
                let msg = format!("{e:#}");
                d.update_operation(op, "error", Some(&msg)).map_err(aerr)?;
                Err(msg)
            }
        }
    })
    .await
}

#[tauri::command]
async fn delete_remote(state: Shared<'_>, path: String) -> Result<(), String> {
    let sftp = current_sftp(&state)?;
    let db = state.lock().map_err(err)?.db.clone();
    blocking(move || {
        sftp.lock().map_err(err)?.delete_file(&path).map_err(aerr)?;
        let d = db.lock().map_err(err)?;
        d.delete_file(&path).ok();
        d.add_operation("delete", "", &path, "done").map_err(aerr)?;
        Ok(())
    })
    .await
}

#[tauri::command]
async fn create_remote_dir(state: Shared<'_>, path: String) -> Result<(), String> {
    let sftp = current_sftp(&state)?;
    blocking(move || sftp.lock().map_err(err)?.mkdir_p(&path).map_err(aerr)).await
}

#[tauri::command]
async fn mount_drive(app: AppHandle, state: Shared<'_>) -> Result<String, String> {
    let (config, password, mount) = {
        let st = state.lock().map_err(err)?;
        (st.config.clone(), resolve_password(&st, "")?, st.mount.clone())
    };
    let res = blocking(move || mount.lock().map_err(err)?.mount(&config, &password).map_err(aerr)).await;
    match &res {
        Ok(mp) => {
            app.emit("mount-changed", json!({ "status": "mounted", "mount_point": mp })).ok();
            notify(&app, "CloudBox", &format!("Диск подключён: {mp}"));
        }
        Err(e) => {
            app.emit("mount-changed", json!({ "status": "error", "mount_point": null, "message": e })).ok();
        }
    }
    res
}

#[tauri::command]
async fn unmount_drive(app: AppHandle, state: Shared<'_>) -> Result<(), String> {
    let mount = state.lock().map_err(err)?.mount.clone();
    blocking(move || mount.lock().map_err(err)?.unmount().map_err(aerr)).await?;
    app.emit("mount-changed", json!({ "status": "unmounted", "mount_point": null })).ok();
    Ok(())
}

#[tauri::command]
async fn get_mount_status(state: Shared<'_>) -> Result<MountStatus, String> {
    let mount = state.lock().map_err(err)?.mount.clone();
    blocking(move || {
        let mut m = mount.lock().map_err(err)?;
        let mounted = m.is_mounted();
        Ok(MountStatus {
            status: if mounted { "mounted".into() } else { "unmounted".into() },
            mount_point: if mounted { m.mount_point() } else { None },
            rclone_available: mount::rclone_available(),
        })
    })
    .await
}

#[tauri::command]
async fn start_sync(app: AppHandle, profile_id: String) -> Result<(), String> {
    spawn_sync(&app, &profile_id)
}

#[tauri::command]
async fn stop_sync(state: Shared<'_>, profile_id: String) -> Result<(), String> {
    if let Some(flag) = state.lock().map_err(err)?.sync_flags.get(&profile_id) {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
async fn get_sync_status(state: Shared<'_>) -> Result<Vec<ProfileStatus>, String> {
    let st = state.lock().map_err(err)?;
    let map = st.profile_status.lock().map_err(err)?;
    Ok(st
        .config
        .profiles
        .iter()
        .map(|p| {
            map.get(&p.id).cloned().unwrap_or(ProfileStatus { profile_id: p.id.clone(), status: "idle".into(), ..Default::default() })
        })
        .collect())
}

#[tauri::command]
async fn get_operations(state: Shared<'_>) -> Result<Vec<Operation>, String> {
    let db = state.lock().map_err(err)?.db.clone();
    let ops = db.lock().map_err(err)?.list_operations(100).map_err(aerr)?;
    Ok(ops)
}

#[tauri::command]
async fn get_storage_info(state: Shared<'_>) -> Result<StorageInfo, String> {
    let sftp = current_sftp(&state)?;
    blocking(move || sftp.lock().map_err(err)?.get_storage_info().map_err(aerr)).await
}

#[tauri::command]
async fn store_password(state: Shared<'_>, password: String) -> Result<(), String> {
    let mut st = state.lock().map_err(err)?;
    let user = st.config.server.username.clone();
    st.password = Some(password.clone());
    secrets::store_password(secrets::SERVICE, &user, &password).map_err(aerr)
}

#[tauri::command]
async fn delete_password(state: Shared<'_>) -> Result<(), String> {
    let mut st = state.lock().map_err(err)?;
    st.password = None;
    secrets::delete_password(secrets::SERVICE, &st.config.server.username).map_err(aerr)
}

#[tauri::command]
async fn check_password_stored(state: Shared<'_>) -> Result<bool, String> {
    let user = state.lock().map_err(err)?.config.server.username.clone();
    Ok(secrets::get_password(secrets::SERVICE, &user).is_ok())
}

// ---------------------------------------------------------------------------
// Background tasks
// ---------------------------------------------------------------------------

/// Emits aggregated transfer speed once per second.
fn start_speed_reporter(app: AppHandle) {
    std::thread::spawn(move || {
        let (mut last_up, mut last_down) = (0u64, 0u64);
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let up = sftp::UPLOADED_BYTES.load(Ordering::Relaxed);
            let down = sftp::DOWNLOADED_BYTES.load(Ordering::Relaxed);
            app.emit("speed-update", json!({ "upload_bytes_s": up - last_up, "download_bytes_s": down - last_down })).ok();
            (last_up, last_down) = (up, down);
        }
    });
}

/// Health check (latency, reconnect) and interval-based scheduler.
fn start_monitor(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(15));
        let state = app.state::<Mutex<AppState>>();
        let (sftp, server, password, profiles, statuses) = match state.lock() {
            Ok(st) => (st.sftp.clone(), st.config.server.clone(), st.password.clone(), st.config.profiles.clone(), st.profile_status.clone()),
            Err(_) => continue,
        };
        let Some(sftp) = sftp else { continue };

        // Skip the ping if the connection is busy with a transfer.
        let ping = match sftp.try_lock() {
            Ok(client) => Some(client.ping()),
            Err(_) => None,
        };
        match ping {
            Some(Ok(latency)) => {
                if let Ok(mut st) = state.lock() {
                    st.latency_ms = latency;
                }
                emit_connection(&app, "connected", latency);
            }
            Some(Err(_)) => {
                emit_connection(&app, "connecting", 0);
                let reconnected = password
                    .as_deref()
                    .and_then(|pw| SftpClient::connect(&server.host, server.ssh_port, &server.username, pw).ok());
                if let Ok(mut st) = state.lock() {
                    match reconnected {
                        Some(client) => {
                            st.sftp = Some(Arc::new(Mutex::new(client)));
                            st.conn_status = "connected".into();
                            emit_connection(&app, "connected", st.latency_ms);
                        }
                        None => {
                            st.sftp = None;
                            st.conn_status = "disconnected".into();
                            emit_connection(&app, "disconnected", 0);
                        }
                    }
                }
            }
            None => {}
        }

        // Interval scheduler.
        let now = chrono::Local::now().timestamp() as u64;
        for p in profiles.iter().filter(|p| p.enabled && p.schedule == "interval") {
            let last = statuses.lock().ok().and_then(|m| m.get(&p.id).map(|s| s.last_run_epoch)).unwrap_or(0);
            if now.saturating_sub(last) >= u64::from(p.interval_minutes.max(1)) * 60 {
                spawn_sync(&app, &p.id).ok();
            }
        }
    });
}

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        w.show().ok();
        w.unminimize().ok();
        w.set_focus().ok();
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Открыть", true, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", "Синхронизировать", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &sync, &sep, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("CloudBox")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "sync" => sync_all(app),
            "quit" => {
                if let Ok(st) = app.state::<Mutex<AppState>>().lock() {
                    st.mount.lock().map(|mut m| m.unmount().ok()).ok();
                }
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = Config::load().unwrap_or_else(|e| {
        eprintln!("config load failed, using defaults: {e:#}");
        Config::default()
    });
    let db = Database::open_default().or_else(|e| {
        eprintln!("database open failed, using in-memory db: {e:#}");
        Database::open_in_memory()
    });
    let state = AppState {
        config,
        password: None,
        sftp: None,
        conn_status: "disconnected".into(),
        latency_ms: 0,
        mount: Arc::new(Mutex::new(MountManager::new())),
        db: Arc::new(Mutex::new(db.expect("cannot open database"))),
        sync_flags: HashMap::new(),
        profile_status: Arc::new(Mutex::new(HashMap::new())),
        watcher: None,
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .manage(Mutex::new(state))
        .setup(|app| {
            setup_tray(app)?;
            start_speed_reporter(app.handle().clone());
            start_monitor(app.handle().clone());
            if std::env::args().any(|a| a == "--minimized") {
                if let Some(w) = app.get_webview_window("main") {
                    w.hide().ok();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let to_tray = window
                    .app_handle()
                    .state::<Mutex<AppState>>()
                    .lock()
                    .map(|st| st.config.ui.minimize_to_tray)
                    .unwrap_or(true);
                if to_tray {
                    api.prevent_close();
                    window.hide().ok();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            connect,
            disconnect,
            test_connection,
            get_connection_status,
            get_remote_home,
            list_remote,
            upload_files,
            download_file,
            delete_remote,
            create_remote_dir,
            mount_drive,
            unmount_drive,
            get_mount_status,
            start_sync,
            stop_sync,
            get_sync_status,
            get_operations,
            get_storage_info,
            store_password,
            delete_password,
            check_password_stored,
        ])
        .run(tauri::generate_context!())
        .expect("error while running CloudBox");
}
