//! Mounts the remote via rclone using MountManager, lists it, unmounts.
//!
//! CB_HOST=... CB_PORT=23 CB_USER=... CB_PASS=... CB_MOUNT=/tmp/cb-mount \
//!   cargo run --example mount_smoke --manifest-path src-tauri/Cargo.toml

use cloudbox_lib::config::Config;
use cloudbox_lib::mount::{rclone_available, MountManager};

fn main() -> anyhow::Result<()> {
    anyhow::ensure!(rclone_available(), "rclone not found");
    let mut cfg = Config::default();
    cfg.server.host = std::env::var("CB_HOST")?;
    cfg.server.ssh_port = std::env::var("CB_PORT")?.parse()?;
    cfg.server.username = std::env::var("CB_USER")?;
    cfg.mount.mount_point = std::env::var("CB_MOUNT").unwrap_or_else(|_| "/tmp/cb-mount".into());
    let pass = std::env::var("CB_PASS")?;

    let mut m = MountManager::new();
    let mp = m.mount(&cfg, &pass)?;
    println!("mounted at {mp}, alive = {}", m.is_mounted());
    std::thread::sleep(std::time::Duration::from_secs(1));
    let names: Vec<String> = std::fs::read_dir(&mp)?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    println!("entries: {names:?}");
    m.unmount()?;
    println!("unmounted, alive = {}", m.is_mounted());
    Ok(())
}
