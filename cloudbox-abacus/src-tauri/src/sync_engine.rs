//! One-way (local → remote) synchronisation engine.
//!
//! Algorithm: scan local folder → compare against the SQLite index and
//! remote `stat` → upload new/changed files → update the index.

use crate::config::{BandwidthConfig, SyncProfile};
use crate::database::Database;
use crate::sftp::{join_remote, SftpClient};
use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncResult {
    pub files_scanned: u32,
    pub files_synced: u32,
    pub files_skipped: u32,
    pub files_failed: u32,
    pub bytes_transferred: u64,
    pub cancelled: bool,
}

#[derive(Debug, Clone)]
pub struct LocalFile {
    pub path: PathBuf,
    /// Relative path with `/` separators.
    pub rel: String,
    pub size: u64,
    pub mtime: u64,
}

/// Event sink: (event name, JSON payload).
pub type Emitter = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

pub struct SyncEngine {
    pub db: Arc<Mutex<Database>>,
    pub sftp: Arc<Mutex<SftpClient>>,
    pub emit: Emitter,
    pub cancel: Arc<AtomicBool>,
    pub bandwidth: BandwidthConfig,
}

/// Matches simple exclude patterns: exact, `*.ext`, `prefix*`, `*contains*`.
pub fn is_excluded(name: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| {
        let p = p.trim();
        if p.is_empty() {
            return false;
        }
        match (p.starts_with('*'), p.ends_with('*') && p.len() > 1) {
            (true, true) => name.contains(&p[1..p.len() - 1]),
            (true, false) => name.ends_with(&p[1..]),
            (false, true) => name.starts_with(&p[..p.len() - 1]),
            (false, false) => name == p,
        }
    }) || name.ends_with(".cloudbox-part")
}

/// Recursively collects regular files under `root`, honouring excludes.
pub fn scan_local(root: &Path, exclude: &[String]) -> Result<Vec<LocalFile>> {
    if !root.is_dir() {
        bail!("local folder does not exist: {}", root.display());
    }
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if is_excluded(&name, exclude) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let path = entry.path();
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                let mtime = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
                out.push(LocalFile { path, rel, size: meta.len(), mtime });
            }
        }
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

impl SyncEngine {
    fn needs_upload(&self, sftp: &SftpClient, file: &LocalFile, remote: &str) -> Result<bool> {
        // Fast path: index says this exact version was already uploaded.
        if let Some(idx) = self.db.lock().unwrap().get_file(remote)? {
            if idx.size as u64 == file.size && idx.mtime == file.mtime.to_string() && idx.sync_status == "synced" {
                return Ok(false);
            }
        }
        // Slow path: compare with the remote file.
        match sftp.stat(remote) {
            Ok(r) if r.size == file.size && r.modified >= file.mtime => {
                self.db.lock().unwrap().upsert_file(remote, &file.path.to_string_lossy(), file.size as i64, &file.mtime.to_string(), None, "synced")?;
                Ok(false)
            }
            _ => Ok(true),
        }
    }

    pub fn sync_profile(&self, profile: &SyncProfile) -> Result<SyncResult> {
        if profile.mode != "one-way-up" {
            bail!("sync mode '{}' is not supported yet (MVP: one-way-up)", profile.mode);
        }
        let root = PathBuf::from(&profile.local_path);
        let files = scan_local(&root, &profile.exclude)?;
        let mut result = SyncResult { files_scanned: files.len() as u32, ..Default::default() };

        // Diff phase.
        let mut queue = Vec::new();
        {
            let sftp = self.sftp.lock().unwrap();
            sftp.mkdir_p(&profile.remote_path)?;
            for f in &files {
                if self.cancel.load(Ordering::Relaxed) {
                    result.cancelled = true;
                    return Ok(result);
                }
                let remote = join_remote(&profile.remote_path, &f.rel);
                if self.needs_upload(&sftp, f, &remote)? {
                    queue.push((f.clone(), remote));
                } else {
                    result.files_skipped += 1;
                }
            }
        }

        let total: u64 = queue.iter().map(|(f, _)| f.size).sum();
        let started = Instant::now();
        let mut uploaded: u64 = 0;
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let limit = self.bandwidth.upload_limit_kbps * 1024;

        // Transfer phase.
        for (file, remote) in queue {
            if self.cancel.load(Ordering::Relaxed) {
                result.cancelled = true;
                break;
            }
            let local_str = file.path.to_string_lossy().into_owned();
            let op_id = self.db.lock().unwrap().add_operation("upload", &local_str, &remote, "in_progress")?;
            let mut on_chunk = |n: u64| {
                uploaded += n;
                let elapsed = started.elapsed().as_secs_f64().max(0.001);
                // Simple bandwidth throttling.
                if limit > 0 {
                    let expected = uploaded as f64 / limit as f64;
                    if expected > elapsed {
                        std::thread::sleep(Duration::from_secs_f64(expected - elapsed));
                    }
                }
                if last_emit.elapsed() >= Duration::from_millis(250) {
                    last_emit = Instant::now();
                    (self.emit)(
                        "sync-progress",
                        json!({
                            "profile_id": profile.id,
                            "file": file.rel,
                            "progress": if total > 0 { uploaded as f32 / total as f32 } else { 1.0 },
                            "speed_bytes_s": (uploaded as f64 / started.elapsed().as_secs_f64().max(0.001)) as u64,
                            "uploaded": uploaded,
                            "total": total,
                        }),
                    );
                }
            };
            let res = self.sftp.lock().unwrap().upload_file_with_progress(&file.path, &remote, &mut on_chunk);
            let db = self.db.lock().unwrap();
            match res {
                Ok(()) => {
                    db.update_operation(op_id, "done", None)?;
                    db.upsert_file(&remote, &local_str, file.size as i64, &file.mtime.to_string(), None, "synced")?;
                    result.files_synced += 1;
                    result.bytes_transferred += file.size;
                }
                Err(e) => {
                    db.update_operation(op_id, "error", Some(&format!("{e:#}")))?;
                    db.upsert_file(&remote, &local_str, file.size as i64, &file.mtime.to_string(), None, "error")?;
                    result.files_failed += 1;
                    (self.emit)("sync-error", json!({ "profile_id": profile.id, "message": format!("{}: {e:#}", file.rel) }));
                }
            }
        }

        {
            let db = self.db.lock().unwrap();
            db.complete_pending_under(&profile.local_path)?;
            db.prune_operations(1000)?;
        }
        (self.emit)(
            "sync-progress",
            json!({ "profile_id": profile.id, "file": "", "progress": 1.0, "speed_bytes_s": 0, "uploaded": uploaded, "total": total }),
        );
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclude_patterns() {
        let p: Vec<String> = ["*.tmp", "~$*", "Thumbs.db", "*cache*"].iter().map(|s| s.to_string()).collect();
        assert!(is_excluded("a.tmp", &p));
        assert!(is_excluded("~$doc.docx", &p));
        assert!(is_excluded("Thumbs.db", &p));
        assert!(is_excluded("my_cache_dir", &p));
        assert!(is_excluded("x.cloudbox-part", &p));
        assert!(!is_excluded("photo.jpg", &p));
    }

    #[test]
    fn scan_finds_nested_files() {
        let dir = std::env::temp_dir().join(format!("cb-scan-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), b"hello").unwrap();
        std::fs::write(dir.join("sub/b.txt"), b"x").unwrap();
        std::fs::write(dir.join("skip.tmp"), b"x").unwrap();
        let files = scan_local(&dir, &["*.tmp".to_string()]).unwrap();
        let rels: Vec<_> = files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(rels, vec!["a.txt", "sub/b.txt"]);
        std::fs::remove_dir_all(dir).ok();
    }
}
