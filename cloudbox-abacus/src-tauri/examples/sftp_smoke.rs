//! End-to-end smoke test against a real SFTP server (no GUI).
//!
//! CB_HOST=u678016.your-storagebox.de CB_PORT=23 CB_USER=u678016 CB_PASS=... \
//!   cargo run --example sftp_smoke --manifest-path src-tauri/Cargo.toml
//!
//! Creates and removes `cloudbox-smoke/` in the remote home directory.

use cloudbox_lib::config::SyncProfile;
use cloudbox_lib::database::Database;
use cloudbox_lib::sftp::{join_remote, SftpClient};
use cloudbox_lib::sync_engine::SyncEngine;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

fn env(k: &str, d: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| d.to_string())
}

fn main() -> anyhow::Result<()> {
    let host = env("CB_HOST", "127.0.0.1");
    let port: u16 = env("CB_PORT", "23").parse()?;
    let user = env("CB_USER", "");
    let pass = env("CB_PASS", "");

    let client = SftpClient::connect(&host, port, &user, &pass)?;
    let home = client.home()?;
    println!("connected, home = {home}, ping = {} ms", client.ping()?);
    match client.get_storage_info() {
        Ok(i) => println!("storage: used {} / total {}", i.used_bytes, i.total_bytes),
        Err(e) => println!("storage info unavailable: {e:#}"),
    }

    let base = join_remote(&home, "cloudbox-smoke");
    client.mkdir_p(&join_remote(&base, "a/b"))?;

    let tmp = std::env::temp_dir().join("cloudbox-smoke");
    std::fs::create_dir_all(tmp.join("sub"))?;
    std::fs::write(tmp.join("hello.txt"), b"hello cloudbox")?;
    std::fs::write(tmp.join("sub/data.bin"), vec![7u8; 1_000_000])?;
    std::fs::write(tmp.join("skip.tmp"), b"x")?;

    client.upload_file(&tmp.join("hello.txt"), &join_remote(&base, "single.txt"))?;
    let back = tmp.join("downloaded.txt");
    client.download_file(&join_remote(&base, "single.txt"), &back)?;
    assert_eq!(std::fs::read(&back)?, b"hello cloudbox");
    std::fs::remove_file(&back)?;
    println!("upload/download roundtrip ok");

    let db = Arc::new(Mutex::new(Database::open_in_memory()?));
    let profile = SyncProfile {
        id: "smoke".into(),
        local_path: tmp.to_string_lossy().into_owned(),
        remote_path: join_remote(&base, "sync"),
        ..Default::default()
    };
    let engine = SyncEngine {
        db: db.clone(),
        sftp: Arc::new(Mutex::new(SftpClient::connect(&host, port, &user, &pass)?)),
        emit: Arc::new(|name, payload| println!("event {name}: {payload}")),
        cancel: Arc::new(AtomicBool::new(false)),
        bandwidth: Default::default(),
    };
    let first = engine.sync_profile(&profile)?;
    println!("first sync: {first:?}");
    assert_eq!(first.files_synced, 2);
    let second = engine.sync_profile(&profile)?;
    println!("second sync: {second:?}");
    assert_eq!(second.files_synced, 0);
    assert_eq!(second.files_skipped, 2);

    let listing = client.list_dir(&join_remote(&base, "sync"))?;
    println!("remote listing: {:?}", listing.iter().map(|e| (&e.name, e.size, e.is_dir)).collect::<Vec<_>>());

    client.delete_file(&base)?;
    assert!(!client.exists(&base));
    std::fs::remove_dir_all(&tmp)?;
    println!("SMOKE OK");
    Ok(())
}
