//! Network drive mounting via an `rclone mount` subprocess.
//!
//! The rclone remote is defined entirely through `RCLONE_CONFIG_*`
//! environment variables, so the password never touches the disk.

use crate::config::Config;
use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const REMOTE: &str = "CLOUDBOX";

pub struct MountManager {
    process: Option<Child>,
    mount_point: Option<String>,
}

impl Default for MountManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Hides the console window for child processes on Windows.
fn command(program: &PathBuf) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

/// Locates rclone: bundled next to the executable first, then `PATH`.
pub fn rclone_binary() -> PathBuf {
    let name = if cfg!(windows) { "rclone.exe" } else { "rclone" };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for candidate in [dir.join(name), dir.join("binaries").join(name), dir.join("resources").join(name)] {
                if candidate.exists() {
                    return candidate;
                }
            }
        }
    }
    PathBuf::from(name)
}

pub fn rclone_available() -> bool {
    command(&rclone_binary())
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Converts a plain password into rclone's obscured form (via stdin).
fn obscure(password: &str) -> Result<String> {
    let mut child = command(&rclone_binary())
        .args(["obscure", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("rclone not found — install rclone or place it next to CloudBox")?;
    child.stdin.take().context("no stdin")?.write_all(password.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("rclone obscure failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Builds `rclone mount` arguments from the config.
pub fn build_mount_args(config: &Config) -> Vec<String> {
    let m = &config.mount;
    let mut args = vec![
        // Empty config path = in-memory config: rclone never writes rclone.conf.
        "--config".to_string(),
        String::new(),
        "mount".to_string(),
        format!("{REMOTE}:{}", m.remote_path),
        m.mount_point.clone(),
        "--vfs-cache-mode".into(),
        m.cache_mode.clone(),
        "--vfs-cache-max-size".into(),
        m.cache_max_size.clone(),
        "--transfers".into(),
        m.transfers.max(1).to_string(),
        "--dir-cache-time".into(),
        "5m".into(),
        "--vfs-read-chunk-size".into(),
        "32M".into(),
        "--buffer-size".into(),
        "32M".into(),
        "--volname".into(),
        "CloudBox".into(),
    ];
    let bw = &config.bandwidth;
    if bw.upload_limit_kbps > 0 || bw.download_limit_kbps > 0 {
        let up = if bw.upload_limit_kbps > 0 { format!("{}k", bw.upload_limit_kbps) } else { "off".into() };
        let down = if bw.download_limit_kbps > 0 { format!("{}k", bw.download_limit_kbps) } else { "off".into() };
        args.push("--bwlimit".into());
        args.push(format!("{up}:{down}"));
    }
    if cfg!(windows) {
        args.push("--network-mode".into());
    }
    args
}

impl MountManager {
    pub fn new() -> Self {
        Self { process: None, mount_point: None }
    }

    pub fn mount_point(&self) -> Option<String> {
        self.mount_point.clone()
    }

    /// Spawns rclone and returns the mount point once the process is alive.
    pub fn mount(&mut self, config: &Config, password: &str) -> Result<String> {
        if self.is_mounted() {
            return Ok(self.mount_point.clone().unwrap_or_default());
        }
        let mp = config.mount.mount_point.clone();
        if mp.trim().is_empty() {
            bail!("mount point is not set");
        }
        if !cfg!(windows) {
            std::fs::create_dir_all(&mp).with_context(|| format!("cannot create mount point {mp}"))?;
        }
        let obscured = obscure(password)?;
        let s = &config.server;
        let mut child = command(&rclone_binary())
            .args(build_mount_args(config))
            .env(format!("RCLONE_CONFIG_{REMOTE}_TYPE"), "sftp")
            .env(format!("RCLONE_CONFIG_{REMOTE}_HOST"), &s.host)
            .env(format!("RCLONE_CONFIG_{REMOTE}_USER"), &s.username)
            .env(format!("RCLONE_CONFIG_{REMOTE}_PORT"), s.ssh_port.to_string())
            .env(format!("RCLONE_CONFIG_{REMOTE}_PASS"), obscured)
            .env(format!("RCLONE_CONFIG_{REMOTE}_SHELL_TYPE"), "unix")
            .env(format!("RCLONE_CONFIG_{REMOTE}_MD5SUM_COMMAND"), "md5sum")
            .env(format!("RCLONE_CONFIG_{REMOTE}_SHA1SUM_COMMAND"), "sha1sum")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to start rclone")?;

        // Give rclone time to connect; if it exits early, report its stderr.
        std::thread::sleep(Duration::from_millis(2500));
        if let Some(status) = child.try_wait()? {
            let mut err = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                stderr.read_to_string(&mut err).ok();
            }
            bail!("rclone exited ({status}): {}", err.trim());
        }
        self.process = Some(child);
        self.mount_point = Some(mp.clone());
        Ok(mp)
    }

    pub fn unmount(&mut self) -> Result<()> {
        if let Some(mut child) = self.process.take() {
            child.kill().ok();
            child.wait().ok();
        }
        // Clean up a stale FUSE mount on Linux/macOS.
        if let Some(mp) = self.mount_point.take() {
            if cfg!(target_os = "linux") {
                for tool in ["fusermount3", "fusermount"] {
                    if Command::new(tool).args(["-uz", &mp]).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false) {
                        break;
                    }
                }
            } else if cfg!(target_os = "macos") {
                Command::new("umount").arg(&mp).stderr(Stdio::null()).status().ok();
            }
        }
        Ok(())
    }

    pub fn is_mounted(&mut self) -> bool {
        match self.process.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }
}

impl Drop for MountManager {
    fn drop(&mut self) {
        self.unmount().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_args_built_from_config() {
        let mut cfg = Config::default();
        cfg.mount.mount_point = "/tmp/cb".into();
        cfg.bandwidth.upload_limit_kbps = 500;
        let args = build_mount_args(&cfg);
        assert_eq!(&args[..2], &["--config".to_string(), String::new()]);
        assert_eq!(args[2], "mount");
        assert_eq!(args[3], "CLOUDBOX:");
        assert!(args.contains(&"--bwlimit".to_string()));
        assert!(args.contains(&"500k:off".to_string()));
        assert!(!args.iter().any(|a| a.contains("pass")));
    }
}
