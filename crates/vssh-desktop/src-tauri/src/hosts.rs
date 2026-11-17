//! Hosts for the window (D6): added, changed and removed in the ssh config
//! and the database together, by the rules of the TUI's form.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use tauri::State;
use vssh_core::config::{home, ssh_config_path};
use vssh_core::ssh::Target;
use vssh_core::store::ssh_config::SshConfig;
use vssh_core::store::{self, Host};
use zeroize::Zeroizing;

use crate::{forwards, Backend, Reply};

/// The TUI's host form, and what the window's adds to it. It is the whole
/// host: an option left empty is taken out of the host's block. What the
/// window's adds is left as it is where the form does not say (`None`).
#[derive(Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Form {
    alias: String,
    hostname: String,
    user: String,
    port: Option<u16>,
    identity_file: String,
    /// The host it is reached through.
    proxy_jump: Option<String>,
    grp: String,
    tags: String,
    notes: String,
    pinned: Option<bool>,
    /// For the vault, which must be unlocked, and nowhere else; an empty
    /// one takes the stored one out. Never read back.
    #[serde(skip_serializing)]
    password: Option<String>,
}

/// The form of a host as it stands: what its own block says, not what a
/// pattern adds to it.
fn form(backend: &Backend, alias: &str) -> Result<Form> {
    let sshc = backend.config_of(alias)?;
    let own = |key| sshc.own(alias, key).to_string();
    let meta = backend.db()?.metas()?.remove(alias).unwrap_or_default();
    Ok(Form {
        alias: alias.to_string(),
        hostname: own("HostName"),
        user: own("User"),
        port: own("Port").parse().ok(),
        identity_file: own("IdentityFile"),
        proxy_jump: Some(own("ProxyJump")),
        grp: meta.grp,
        tags: meta.tags,
        notes: meta.notes,
        pinned: Some(meta.pinned),
        password: None,
    })
}

/// What `hosts_update` takes for the host, filled in with what it is now.
#[tauri::command(async)]
pub fn hosts_form(alias: String, backend: State<'_, Backend>) -> Reply<Form> {
    Ok(form(&backend, &alias)?)
}

/// Saves the form as a new host, or over the host `old`.
fn save(backend: &Backend, old: Option<&str>, mut form: Form) -> Result<Host> {
    let password = form.password.take().map(Zeroizing::new);
    // Before anything is written: the password has nowhere else to go.
    let locked = backend.vault.lock().unwrap().is_none();
    ensure!(password.is_none() || !locked, "the vault is locked");
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
    if let Some(jump) = &form.proxy_jump {
        sshc.set(alias, "ProxyJump", jump.trim());
    }
    backend.save(&sshc)?;

    let db = backend.db()?;
    if let Some(old) = old.filter(|_| renamed) {
        // They run under the old name, which nothing lists any more.
        forwards::stop_all(backend, old);
        db.rename(old, alias)?;
    }
    // What the form does not show stays: how often it was used.
    let mut meta = db.metas()?.remove(alias).unwrap_or_default();
    let text = |field: &str| field.trim().to_string();
    (meta.grp, meta.tags, meta.notes) = (text(&form.grp), text(&form.tags), text(&form.notes));
    meta.pinned = form.pinned.unwrap_or(meta.pinned);
    db.set_meta(alias, &meta)?;

    if let Some(vault) = backend.vault.lock().unwrap().as_mut() {
        // Its password is the host's, whatever the host is called.
        let kept = old.filter(|_| renamed).and_then(|old| vault.get(old));
        if let Some(mut kept) = kept.map(|kept| Zeroizing::new(kept.to_string())) {
            vault.set(alias, &mut kept)?;
            vault.set(old.unwrap_or_default(), &mut String::new())?;
        }
        if let Some(mut password) = password {
            vault.set(alias, &mut password)?;
        }
    }
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
    backend.db()?.delete(alias)?;
    // Or the next host of that name would be logged in to with it.
    // shortcut: a locked vault keeps the password; drop it at the next
    // unlock if hosts are deleted and their names reused with the vault shut.
    if let Some(vault) = backend.vault.lock().unwrap().as_mut() {
        vault.set(alias, &mut String::new())?;
    }
    Ok(())
}

/// Removes the host with its metadata, its own snippets and its password
/// in the unlocked vault, and stops its forwards.
#[tauri::command(async)]
pub fn hosts_delete(alias: String, backend: State<'_, Backend>) -> Reply<()> {
    Ok(delete(&backend, &alias)?)
}

/// Another ssh config file: `~/.ssh/config` if no path is given.
fn theirs(path: Option<PathBuf>) -> Result<SshConfig> {
    let path = path.unwrap_or_else(ssh_config_path);
    // Typed by a person, with no shell to read the `~`.
    let path = match path.strip_prefix("~") {
        Ok(rest) => home().join(rest),
        Err(_) => path,
    };
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    Ok(SshConfig::parse(&text))
}

fn preview(backend: &Backend, path: Option<PathBuf>) -> Result<Vec<Host>> {
    let ours = backend.ssh_config()?.hosts();
    let mut new = store::hosts(&theirs(path)?, &HashMap::new());
    new.retain(|host| !ours.contains(&host.alias));
    Ok(new)
}

/// The hosts `hosts_import_ssh_config` would take over from that file:
/// those the app does not have yet, as they are reached there.
#[tauri::command(async)]
pub fn hosts_import_preview(
    path: Option<PathBuf>,
    backend: State<'_, Backend>,
) -> Reply<Vec<Host>> {
    Ok(preview(&backend, path)?)
}

fn import(
    backend: &Backend,
    path: Option<PathBuf>,
    only: Option<Vec<String>>,
) -> Result<Vec<String>> {
    let mut theirs = theirs(path)?;
    for alias in theirs.hosts() {
        if only.as_ref().is_some_and(|only| !only.contains(&alias)) {
            theirs.remove_host(&alias);
        }
    }
    let mut sshc = backend.ssh_config()?;
    let added = sshc.import(&theirs);
    if !added.is_empty() {
        backend.save(&sshc)?;
        backend.db()?.import(&added)?;
    }
    Ok(added)
}

/// Takes over the hosts of another ssh config file (`~/.ssh/config` if no
/// path is given) that the app does not have yet, `ProxyJump` and all, and
/// returns their aliases. `only`: of those, the ones that were chosen. A
/// host the app has is never written over.
#[tauri::command(async)]
pub fn hosts_import_ssh_config(
    path: Option<PathBuf>,
    only: Option<Vec<String>>,
    backend: State<'_, Backend>,
) -> Reply<Vec<String>> {
    Ok(import(&backend, path, only)?)
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
