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

/// The hosts the unlocked vault has a password for; none while it is
/// locked. The names only: no command hands a password back.
#[tauri::command(async)]
pub fn vault_hosts(backend: State<'_, Backend>) -> Vec<String> {
    let vault = backend.vault.lock().unwrap();
    let names = vault.iter().flat_map(Vault::names);
    names.map(str::to_string).collect()
}

/// Drops the passwords of hosts that are gone. A host deleted while the
/// vault was shut (here or in the TUI) left its password in it, and the next
/// host of that name would be logged in to with it.
// shortcut: a name deleted and given to a new host before the vault is next
// unlocked keeps the old password; keep a list of the names deleted while it
// is shut (a table of the database) if that is ever met.
fn forget_gone(backend: &Backend, vault: &mut Vault) {
    // A config that cannot be read, or lists no host at all, is more likely
    // a file that is not where it was than every host deleted: nothing goes.
    let hosts = backend.ssh_config().map(|sshc| sshc.hosts());
    let Some(hosts) = hosts.ok().filter(|hosts| !hosts.is_empty()) else {
        return;
    };
    let gone = |name: &&str| !hosts.iter().any(|host| host == name);
    let gone: Vec<String> = vault.names().filter(gone).map(str::to_string).collect();
    for name in gone {
        match vault.set(&name, &mut String::new()) {
            Ok(()) => eprintln!("vssh: vault: dropped the password of {name}: no such host"),
            Err(e) => eprintln!("vssh: vault: cannot drop the password of {name}: {e:#}"),
        }
    }
}

fn unlock(backend: &Backend, passphrase: &str) -> Result<()> {
    let path = path(backend)?;
    let mut vault = match path.exists() {
        true => Vault::open(&path, passphrase)?,
        false => Vault::create(&path, passphrase)?,
    };
    forget_gone(backend, &mut vault);
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
    forget_gone(backend, &mut vault);
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
