//! Application configuration stored in `~/.cloudbox/config.toml`.
//! Passwords are never stored here — see `secrets.rs`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,
    pub username: String,
    pub ssh_port: u16,
    /// Transport protocol. Only "sftp" is implemented in the MVP.
    pub protocol: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "u678016.your-storagebox.de".into(),
            username: "u678016".into(),
            ssh_port: 23,
            protocol: "sftp".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MountConfig {
    pub enabled: bool,
    pub mount_point: String,
    /// Remote directory that is exposed as the drive root.
    pub remote_path: String,
    pub cache_mode: String,
    pub cache_max_size: String,
    pub transfers: u32,
}

impl Default for MountConfig {
    fn default() -> Self {
        let mount_point = if cfg!(windows) {
            "Z:".to_string()
        } else {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("CloudBox")
                .to_string_lossy()
                .into_owned()
        };
        Self {
            enabled: false,
            mount_point,
            remote_path: String::new(),
            cache_mode: "full".into(),
            cache_max_size: "5G".into(),
            transfers: 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncProfile {
    pub id: String,
    pub name: String,
    pub local_path: String,
    pub remote_path: String,
    /// "one-way-up" (local → remote). Other modes are planned.
    pub mode: String,
    /// "manual" | "realtime" | "interval"
    pub schedule: String,
    pub interval_minutes: u32,
    pub enabled: bool,
    /// Simple patterns: exact name, "*.ext" or "prefix*".
    pub exclude: Vec<String>,
}

impl Default for SyncProfile {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Новый профиль".into(),
            local_path: String::new(),
            remote_path: "/backup".into(),
            mode: "one-way-up".into(),
            schedule: "manual".into(),
            interval_minutes: 60,
            enabled: true,
            exclude: vec![".DS_Store".into(), "Thumbs.db".into(), "*.tmp".into(), "~$*".into()],
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BandwidthConfig {
    /// KiB/s, 0 = unlimited
    pub upload_limit_kbps: u64,
    /// KiB/s, 0 = unlimited
    pub download_limit_kbps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub autostart: bool,
    pub minimize_to_tray: bool,
    pub language: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self { autostart: false, minimize_to_tray: true, language: "ru".into() }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub mount: MountConfig,
    pub bandwidth: BandwidthConfig,
    pub ui: UiConfig,
    pub profiles: Vec<SyncProfile>,
}

/// `~/.cloudbox`, created on demand.
pub fn app_dir() -> Result<PathBuf> {
    let dir = dirs::home_dir().context("home directory not found")?.join(".cloudbox");
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    Ok(dir)
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        Ok(app_dir()?.join("config.toml"))
    }

    /// Loads the config, writing defaults on first start.
    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            let cfg = Config::default();
            cfg.save()?;
            return Ok(cfg);
        }
        let text = std::fs::read_to_string(&path)?;
        toml::from_str(&text).with_context(|| format!("invalid config {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        let text = toml::to_string_pretty(self)?;
        std::fs::write(Self::path()?, text)?;
        Ok(())
    }

    pub fn profile(&self, id: &str) -> Option<&SyncProfile> {
        self.profiles.iter().find(|p| p.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_toml() {
        let mut cfg = Config::default();
        cfg.profiles.push(SyncProfile { id: "p1".into(), ..Default::default() });
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.server.ssh_port, 23);
        assert_eq!(back.profiles.len(), 1);
    }

    #[test]
    fn partial_toml_uses_defaults() {
        let cfg: Config = toml::from_str("[server]\nhost = \"x\"\n").unwrap();
        assert_eq!(cfg.server.host, "x");
        assert_eq!(cfg.server.ssh_port, 23);
        assert_eq!(cfg.mount.transfers, 4);
    }
}
