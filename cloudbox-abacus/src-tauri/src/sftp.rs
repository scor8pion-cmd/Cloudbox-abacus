//! SFTP transport built on libssh2 (`ssh2` crate).

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use ssh2::{FileStat, Session, Sftp};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Global transfer counters used to compute live speed for the UI.
pub static UPLOADED_BYTES: AtomicU64 = AtomicU64::new(0);
pub static DOWNLOADED_BYTES: AtomicU64 = AtomicU64::new(0);

const CHUNK: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    /// Unix timestamp (seconds)
    pub modified: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StorageInfo {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

pub struct SftpClient {
    session: Session,
    sftp: Sftp,
}

/// Joins remote path segments with `/`.
pub fn join_remote(base: &str, name: &str) -> String {
    if base.is_empty() || base == "." {
        return name.to_string();
    }
    format!("{}/{}", base.trim_end_matches('/'), name.trim_start_matches('/'))
}

fn parent_remote(path: &str) -> Option<&str> {
    let trimmed = path.trim_end_matches('/');
    trimmed.rfind('/').map(|i| if i == 0 { "/" } else { &trimmed[..i] })
}

impl SftpClient {
    pub fn connect(host: &str, port: u16, username: &str, password: &str) -> Result<SftpClient> {
        let addr = (host, port)
            .to_socket_addrs()
            .with_context(|| format!("cannot resolve {host}"))?
            .next()
            .ok_or_else(|| anyhow!("no address for {host}"))?;
        let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(10))
            .with_context(|| format!("TCP connection to {host}:{port} failed"))?;
        tcp.set_nodelay(true).ok();

        let mut session = Session::new()?;
        session.set_tcp_stream(tcp);
        session.set_timeout(30_000);
        session.handshake().context("SSH handshake failed")?;
        session
            .userauth_password(username, password)
            .context("authentication failed (wrong username or password?)")?;
        if !session.authenticated() {
            bail!("authentication failed");
        }
        session.set_keepalive(true, 30);
        let sftp = session.sftp().context("SFTP subsystem unavailable")?;
        Ok(SftpClient { session, sftp })
    }

    fn entry(path: &str, name: String, st: &FileStat) -> FileEntry {
        FileEntry {
            name,
            path: path.to_string(),
            size: st.size.unwrap_or(0),
            is_dir: st.is_dir(),
            modified: st.mtime.unwrap_or(0),
        }
    }

    /// Absolute path of the remote home directory.
    pub fn home(&self) -> Result<String> {
        Ok(self.sftp.realpath(Path::new("."))?.to_string_lossy().into_owned())
    }

    pub fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>> {
        let mut entries: Vec<FileEntry> = self
            .sftp
            .readdir(Path::new(path))
            .with_context(|| format!("cannot list {path}"))?
            .into_iter()
            .filter_map(|(p, st)| {
                let name = p.file_name()?.to_string_lossy().into_owned();
                if name == "." || name == ".." {
                    return None;
                }
                Some(Self::entry(&join_remote(path, &name), name, &st))
            })
            .collect();
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        Ok(entries)
    }

    pub fn stat(&self, path: &str) -> Result<FileEntry> {
        let st = self.sftp.stat(Path::new(path)).with_context(|| format!("cannot stat {path}"))?;
        let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or(path).to_string();
        Ok(Self::entry(path, name, &st))
    }

    pub fn exists(&self, path: &str) -> bool {
        self.sftp.stat(Path::new(path)).is_ok()
    }

    pub fn mkdir(&self, path: &str) -> Result<()> {
        self.sftp.mkdir(Path::new(path), 0o755).with_context(|| format!("cannot create {path}"))
    }

    /// Creates a directory and all missing parents.
    pub fn mkdir_p(&self, path: &str) -> Result<()> {
        if path.is_empty() || path == "/" || path == "." || self.exists(path) {
            return Ok(());
        }
        if let Some(parent) = parent_remote(path) {
            self.mkdir_p(parent)?;
        }
        match self.mkdir(path) {
            Ok(()) => Ok(()),
            Err(_) if self.exists(path) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub fn upload_file(&self, local: &Path, remote: &str) -> Result<()> {
        self.upload_file_with_progress(local, remote, &mut |_| {})
    }

    /// Uploads via a temporary `.cloudbox-part` file and renames atomically.
    /// `on_chunk` receives the number of bytes written for each chunk.
    pub fn upload_file_with_progress(&self, local: &Path, remote: &str, on_chunk: &mut dyn FnMut(u64)) -> Result<()> {
        let mut src = std::fs::File::open(local).with_context(|| format!("cannot open {}", local.display()))?;
        if let Some(parent) = parent_remote(remote) {
            self.mkdir_p(parent)?;
        }
        let tmp = format!("{remote}.cloudbox-part");
        {
            let mut dst = self.sftp.create(Path::new(&tmp)).with_context(|| format!("cannot create {tmp}"))?;
            let mut buf = vec![0u8; CHUNK];
            loop {
                let n = src.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                dst.write_all(&buf[..n])?;
                UPLOADED_BYTES.fetch_add(n as u64, Ordering::Relaxed);
                on_chunk(n as u64);
            }
            dst.flush()?;
        }
        if self.exists(remote) {
            self.sftp.unlink(Path::new(remote)).ok();
        }
        self.sftp
            .rename(Path::new(&tmp), Path::new(remote), None)
            .with_context(|| format!("cannot rename {tmp} -> {remote}"))?;

        // Preserve local mtime so later comparisons are stable.
        if let Ok(mtime) = std::fs::metadata(local).and_then(|m| m.modified()) {
            if let Ok(secs) = mtime.duration_since(std::time::UNIX_EPOCH) {
                let st = FileStat { size: None, uid: None, gid: None, perm: None, atime: Some(secs.as_secs()), mtime: Some(secs.as_secs()) };
                self.sftp.setstat(Path::new(remote), st).ok();
            }
        }
        Ok(())
    }

    pub fn download_file(&self, remote: &str, local: &Path) -> Result<()> {
        let mut src = self.sftp.open(Path::new(remote)).with_context(|| format!("cannot open {remote}"))?;
        if let Some(parent) = local.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut dst = std::fs::File::create(local).with_context(|| format!("cannot create {}", local.display()))?;
        let mut buf = vec![0u8; CHUNK];
        loop {
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n])?;
            DOWNLOADED_BYTES.fetch_add(n as u64, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Deletes a file, or a directory recursively.
    pub fn delete_file(&self, remote: &str) -> Result<()> {
        let st = self.sftp.lstat(Path::new(remote)).with_context(|| format!("cannot stat {remote}"))?;
        if st.is_dir() {
            for child in self.list_dir(remote)? {
                self.delete_file(&child.path)?;
            }
            self.sftp.rmdir(Path::new(remote)).with_context(|| format!("cannot remove {remote}"))
        } else {
            self.sftp.unlink(Path::new(remote)).with_context(|| format!("cannot delete {remote}"))
        }
    }

    /// Runs a command over SSH (Hetzner allows a restricted command set).
    fn exec(&self, cmd: &str) -> Result<String> {
        let mut ch = self.session.channel_session()?;
        ch.exec(cmd)?;
        let mut out = String::new();
        ch.read_to_string(&mut out)?;
        ch.wait_close().ok();
        Ok(out)
    }

    /// Quota via `df` (supported by the Hetzner Storage Box restricted shell).
    pub fn get_storage_info(&self) -> Result<StorageInfo> {
        let out = self.exec("df -k")?;
        parse_df(&out).ok_or_else(|| anyhow!("unexpected df output: {out}"))
    }

    /// Round-trip latency of a cheap SFTP request.
    pub fn ping(&self) -> Result<u64> {
        let start = Instant::now();
        self.sftp.stat(Path::new("."))?;
        Ok(start.elapsed().as_millis() as u64)
    }
}

/// Parses `df -k` output (1K blocks): sums the first data line.
fn parse_df(out: &str) -> Option<StorageInfo> {
    let line = out.lines().skip(1).find(|l| !l.trim().is_empty())?;
    let nums: Vec<u64> = line.split_whitespace().filter_map(|t| t.parse().ok()).collect();
    if nums.len() < 3 {
        return None;
    }
    Some(StorageInfo { total_bytes: nums[0] * 1024, used_bytes: nums[1] * 1024, available_bytes: nums[2] * 1024 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn df_parsing() {
        let out = "Filesystem     1K-blocks     Used  Available Use% Mounted on\nu678016  5368709120 82800000 5285909120   2% /home\n";
        let info = parse_df(out).unwrap();
        assert_eq!(info.total_bytes, 5368709120 * 1024);
        assert_eq!(info.used_bytes, 82800000 * 1024);
    }

    #[test]
    fn remote_paths() {
        assert_eq!(join_remote("/home", "a"), "/home/a");
        assert_eq!(join_remote("/", "a"), "/a");
        assert_eq!(parent_remote("/a/b/c"), Some("/a/b"));
        assert_eq!(parent_remote("/a"), Some("/"));
    }
}
