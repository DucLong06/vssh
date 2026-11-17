//! The vault for the window (D6): the TUI's own file. Once it is unlocked
//! its passwords go with every connection to the host they are stored for;
//! no command hands a password back to the window.

use std::path::PathBuf;

use anyhow::{Context, Result};
use tauri::State;
use vssh_core::vault::Vault;
use zeroize::Zeroizing;

use crate::{Backend, Reply};

fn path(backend: &Backend) -> Result<PathBuf> {
    Ok(backend.data_dir()?.join("vault"))
}

/// `none` (there is no vault yet), `locked` or `unlocked`.
#[tauri::command(async)]
pub fn vault_state(backend: State<'_, Backend>) -> &'static str {
    let unlocked = backend.vault.lock().unwrap().is_some();
    match (unlocked, backend.data_dir.join("vault").exists()) {
        (true, _) => "unlocked",
        (false, true) => "locked",
        (false, false) => "none",
    }
}

fn unlock(backend: &Backend, passphrase: &str) -> Result<()> {
    let path = path(backend)?;
    let vault = match path.exists() {
        true => Vault::open(&path, passphrase)?,
        false => Vault::create(&path, passphrase)?,
    };
    *backend.vault.lock().unwrap() = Some(vault);
    Ok(())
}

/// Unlocks the vault. If there is none yet (`vault_state` says so), this
/// makes it, with this passphrase: ask for it twice.
#[tauri::command(async)]
pub fn vault_unlock(passphrase: String, backend: State<'_, Backend>) -> Reply<()> {
    let passphrase = Zeroizing::new(passphrase);
    Ok(unlock(&backend, &passphrase)?)
}

fn change(backend: &Backend, old: &str, new: &str) -> Result<()> {
    // Opened afresh: the old passphrase is asked for even of an open vault.
    let mut vault = Vault::open(&path(backend)?, old)?;
    vault.change_passphrase(new)?;
    *backend.vault.lock().unwrap() = Some(vault);
    Ok(())
}

/// Seals the vault under a new passphrase; it is unlocked afterwards.
#[tauri::command(async)]
pub fn vault_pass_change(old: String, new: String, backend: State<'_, Backend>) -> Reply<()> {
    let (old, new) = (Zeroizing::new(old), Zeroizing::new(new));
    Ok(change(&backend, &old, &new)?)
}

fn export(backend: &Backend, to: PathBuf) -> Result<()> {
    let vault = backend.vault.lock().unwrap();
    vault.as_ref().context("the vault is locked")?.export(&to)
}

/// Writes a copy of the unlocked vault to a new file, sealed under the
/// same passphrase. Nothing leaves the vault in the clear.
#[tauri::command(async)]
pub fn vault_export(path: PathBuf, backend: State<'_, Backend>) -> Reply<()> {
    Ok(export(&backend, path)?)
}
