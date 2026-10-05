//! Thin wrapper over the OS keychain (Windows Credential Manager,
//! macOS Keychain, Linux Secret Service).

use anyhow::{Context, Result};

pub const SERVICE: &str = "cloudbox";

pub fn store_password(service: &str, username: &str, password: &str) -> Result<()> {
    keyring::Entry::new(service, username)?
        .set_password(password)
        .context("failed to store password in system keychain")
}

pub fn get_password(service: &str, username: &str) -> Result<String> {
    keyring::Entry::new(service, username)?
        .get_password()
        .context("password not found in system keychain")
}

pub fn delete_password(service: &str, username: &str) -> Result<()> {
    match keyring::Entry::new(service, username)?.delete_password() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
