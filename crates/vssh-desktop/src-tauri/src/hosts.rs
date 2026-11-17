//! Hosts for the window (D6): added, changed and removed in the ssh config
//! and the database together, by the rules of the TUI's form.

use std::path::PathBuf;

use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use tauri::State;
use vssh_core::config::ssh_config_path;
use vssh_core::ssh::Target;
use vssh_core::store::ssh_config::SshConfig;
use vssh_core::store::Host;
use zeroize::Zeroizing;

use crate::{forwards, Backend, Reply};

/// The TUI's host form. It is the whole host: an option left empty is
/// taken out of the host's block.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Form {
    alias: String,
    hostname: String,
    user: String,
    port: Option<u16>,
    identity_file: String,
    grp: String,
    tags: String,
    notes: String,
}

/// Saves the form as a new host, or over the host `old`.
fn save(backend: &Backend, old: Option<&str>, form: Form) -> Result<Host> {
    let alias = form.alias.trim();
    let bad = |c: char| c.is_whitespace() || "*?!#\"=".contains(c);
    ensure!(
        !alias.is_empty() && !alias.contains(bad),
        "alias must be one word without wildcards"
    );
    let mut sshc = match old {
        Some(old) => backend.config_of(old)?,
        None => backend.ssh_config()?,
    };
    let renamed = old != Some(alias);
    let taken = sshc.hosts().iter().any(|host| host == alias);
    ensure!(!renamed || !taken, "host {alias} already exists");
    match old {
        Some(old) if renamed => sshc.rename_host(old, alias),
        Some(_) => {}
        None => sshc.add_host(alias),
    }
    let port = form.port.map(|port| port.to_string()).unwrap_or_default();
    let options = [
        ("HostName", &form.hostname),
        ("User", &form.user),
        ("Port", &port),
        ("IdentityFile", &form.identity_file),
    ];
    for (key, value) in options {
        sshc.set(alias, key, value.trim());
    }
    backend.save(&sshc)?;

    let db = backend.db()?;
    if let Some(old) = old.filter(|_| renamed) {
        // They run under the old name, which nothing lists any more.
        forwards::stop_all(backend, old);
        db.rename(old, alias)?;
    }
    // What the form does not show stays: the pin, and how often it was used.
    let mut meta = db.metas()?.remove(alias).unwrap_or_default();
    let text = |field: &str| field.trim().to_string();
    (meta.grp, meta.tags, meta.notes) = (text(&form.grp), text(&form.tags), text(&form.notes));
    db.set_meta(alias, &meta)?;
    let saved = backend
        .hosts()?
        .into_iter()
        .find(|host| host.alias == alias);
    saved.context("the host was saved and is not in the config")
}

#[tauri::command(async)]
pub fn hosts_add(form: Form, backend: State<'_, Backend>) -> Reply<Host> {
    Ok(save(&backend, None, form)?)
}

/// Saves the form over the host `alias`; another alias in the form renames it.
#[tauri::command(async)]
pub fn hosts_update(alias: String, form: Form, backend: State<'_, Backend>) -> Reply<Host> {
    Ok(save(&backend, Some(&alias), form)?)
}

fn delete(backend: &Backend, alias: &str) -> Result<()> {
    let mut sshc = backend.config_of(alias)?;
    forwards::stop_all(backend, alias);
    sshc.remove_host(alias);
    backend.save(&sshc)?;
    Ok(backend.db()?.delete(alias)?)
}

/// Removes the host with its metadata and its own snippets, and stops its
/// forwards.
#[tauri::command(async)]
pub fn hosts_delete(alias: String, backend: State<'_, Backend>) -> Reply<()> {
    Ok(delete(&backend, &alias)?)
}

fn import(backend: &Backend, path: Option<PathBuf>) -> Result<Vec<String>> {
    let path = path.unwrap_or_else(ssh_config_path);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    let mut sshc = backend.ssh_config()?;
    let added = sshc.import(&SshConfig::parse(&text));
    if !added.is_empty() {
        backend.save(&sshc)?;
        backend.db()?.import(&added)?;
    }
    Ok(added)
}

/// Takes over the hosts of another ssh config file (`~/.ssh/config` if no
/// path is given) that the app does not have yet, `ProxyJump` and all, and
/// returns their aliases.
#[tauri::command(async)]
pub fn hosts_import_ssh_config(
    path: Option<PathBuf>,
    backend: State<'_, Backend>,
) -> Reply<Vec<String>> {
    Ok(import(&backend, path)?)
}

fn quick_save(backend: &Backend, host: &str, password: Option<String>) -> Result<String> {
    let password = password.map(Zeroizing::new).filter(|p| !p.is_empty());
    let target = Target::parse(host)?;
    let mut vault = backend.vault.lock().unwrap();
    // Before anything is written: the password has nowhere else to go.
    ensure!(password.is_none() || vault.is_some(), "the vault is locked");
    let mut sshc = backend.ssh_config()?;
    let alias = target.save_to(&mut sshc)?;
    backend.save(&sshc)?;
    backend.db()?.import(std::slice::from_ref(&alias))?;
    if let (Some(vault), Some(mut password)) = (vault.as_mut(), password) {
        vault.set(&alias, &mut password)?;
    }
    Ok(alias)
}

/// Saves a quick connect (`[user@]host[:port]`) as a host and returns the
/// alias it got. Its password, if one is given, goes into the vault and
/// nowhere else; the vault must be unlocked for that.
#[tauri::command(async)]
pub fn host_quick_save(
    host: String,
    password: Option<String>,
    backend: State<'_, Backend>,
) -> Reply<String> {
    Ok(quick_save(&backend, &host, password)?)
}
