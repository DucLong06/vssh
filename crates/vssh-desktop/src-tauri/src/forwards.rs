//! Port forwards for the window (D5). A forward is a line of its host's
//! block in the ssh config, as for `ssh` and the TUI; started from here it
//! runs until it fails or is stopped, and the window hears on
//! `forward-state` when it listens and when it has ended.

use std::sync::atomic::Ordering;

use anyhow::{ensure, Context, Result};
use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use vssh_core::conn_err::{ConnError, Fail};
use vssh_core::store::ssh_config::{Forward, SshConfig, FORWARD_KEYS};

use crate::{Backend, Reply};

/// A forward that was started and has not ended.
pub struct Running {
    /// Tells this run from a later one of the same line.
    id: u64,
    task: JoinHandle<()>,
    fwd: Forward,
    listening: bool,
}

/// One forward of a host: its parts, the line they are in the config
/// (which is its name for `forward_toggle` and `forward_kill`), and
/// whether it runs: `off`, `starting` or `on`.
#[derive(Serialize)]
pub struct Row {
    #[serde(flatten)]
    fwd: Forward,
    value: String,
    state: &'static str,
}

/// What `forward-state` carries: `on` once a forward listens, `off` when it
/// has ended by itself, with why.
#[derive(Clone, Serialize)]
struct Change {
    host: String,
    kind: char,
    value: String,
    state: &'static str,
    error: Option<ConnError>,
}

fn key(host: &str, kind: char, value: &str) -> String {
    format!("{host} {kind} {value}")
}

/// The forwards in a host's own block, each with the line it is.
fn saved(sshc: &SshConfig, host: &str) -> Vec<(Forward, String)> {
    let lines = FORWARD_KEYS.iter().flat_map(|(kind, option)| {
        let values = sshc.own_all(host, option).into_iter();
        values.filter_map(|value| Some((Forward::parse(*kind, value)?, value.to_string())))
    });
    lines.collect()
}

/// True if the two would listen on the same port of the same machine: a
/// remote forward listens on the host, the other two on this machine.
fn clash(a: &Forward, b: &Forward) -> bool {
    a.port == b.port && (a.kind == 'R') == (b.kind == 'R')
}

fn in_use(fwd: &Forward) -> anyhow::Error {
    let addr = format!("{}:{}", fwd.bind, fwd.port);
    Fail::PortInUse { addr }.into()
}

fn list(backend: &Backend, host: &str) -> Result<Vec<Row>> {
    let sshc = backend.config_of(host)?;
    let running = backend.forwards.lock().unwrap();
    let rows = saved(&sshc, host).into_iter().map(|(fwd, value)| {
        let state = match running.get(&key(host, fwd.kind, &value)) {
            None => "off",
            Some(run) if run.listening => "on",
            Some(_) => "starting",
        };
        Row { fwd, value, state }
    });
    Ok(rows.collect())
}

#[tauri::command(async)]
pub fn forwards_list(host: String, backend: State<'_, Backend>) -> Reply<Vec<Row>> {
    Ok(list(&backend, &host)?)
}

fn add(
    backend: &Backend,
    host: &str,
    kind: char,
    port: u16,
    remote: Option<String>,
) -> Result<Row> {
    let mut sshc = backend.config_of(host)?;
    let spec = format!("{port} {}", remote.unwrap_or_default());
    let fwd = Forward::parse(kind.to_ascii_uppercase(), &spec)
        .filter(|fwd| fwd.port != 0)
        .context(
            "a forward is L or R with a port and where it leads (host:port), or D with a port",
        )?;
    let taken = saved(&sshc, host).iter().any(|(old, _)| clash(old, &fwd));
    if taken {
        return Err(in_use(&fwd));
    }
    let value = fwd.value();
    sshc.add(host, fwd.key(), &value);
    backend.save(&sshc)?;
    let state = "off";
    Ok(Row { fwd, value, state })
}

/// Adds a forward to the host's block: `kind` is `L`, `R` or `D`,
/// `local_port` where it listens, `remote` where it leads (`host:port`;
/// nothing for `D`). It is not started.
#[tauri::command(async)]
pub fn forward_add(
    host: String,
    kind: char,
    local_port: u16,
    remote: Option<String>,
    backend: State<'_, Backend>,
) -> Reply<Row> {
    Ok(add(&backend, &host, kind, local_port, remote)?)
}

fn toggle<R: Runtime>(
    app: AppHandle<R>,
    backend: &Backend,
    host: String,
    kind: char,
    value: String,
) -> Result<bool> {
    let sshc = backend.config_of(&host)?;
    let key = key(&host, kind, &value);
    let mut running = backend.forwards.lock().unwrap();
    if let Some(run) = running.remove(&key) {
        run.task.abort();
        return Ok(false);
    }
    let fwd = saved(&sshc, &host).into_iter().find_map(|(fwd, line)| {
        let this = fwd.kind == kind && line == value;
        this.then_some(fwd)
    });
    let fwd = fwd.with_context(|| format!("{host} has no forward {kind} {value}"))?;
    // Two forwards of this app on one local port: said before connecting.
    let held = running.values().any(|run| clash(&run.fwd, &fwd));
    if held && kind != 'R' {
        return Err(in_use(&fwd));
    }
    let (target, password) = backend.login(&host)?;
    let id = backend.jobs.fetch_add(1, Ordering::Relaxed);
    let change = |state, error| Change {
        host: host.clone(),
        kind,
        value: value.clone(),
        state,
        error,
    };

    let (on, at, window) = (change("on", None), key.clone(), app.clone());
    let ready = move || {
        let backend = window.state::<Backend>();
        let mut running = backend.forwards.lock().unwrap();
        let ours = running.get_mut(&at).filter(|run| run.id == id);
        let ours = ours.map(|run| run.listening = true).is_some();
        drop(running);
        if ours {
            let _ = window.emit("forward-state", on);
        }
    };
    let run = backend
        .net
        .forward(target, password, fwd.clone(), Box::new(ready));
    let (off, at) = (change("off", None), key.clone());
    // Runs once the lock is let go of below, so its entry is there by then.
    let task = tauri::async_runtime::spawn(async move {
        let error = run.await.err();
        let backend = app.state::<Backend>();
        let mut running = backend.forwards.lock().unwrap();
        let ours = running.get(&at).is_some_and(|run| run.id == id);
        if ours {
            running.remove(&at);
        }
        drop(running);
        if ours {
            let _ = app.emit("forward-state", Change { error, ..off });
        }
    });
    let listening = false;
    running.insert(
        key,
        Running {
            id,
            task,
            fwd,
            listening,
        },
    );
    Ok(true)
}

/// Starts a forward of the config, or stops it if it runs. True if it is
/// now starting; `forward-state` says when it listens.
#[tauri::command(async)]
pub fn forward_toggle<R: Runtime>(
    app: AppHandle<R>,
    host: String,
    kind: char,
    value: String,
    backend: State<'_, Backend>,
) -> Reply<bool> {
    Ok(toggle(app, &backend, host, kind, value)?)
}

fn kill(backend: &Backend, host: &str, kind: char, value: &str) -> Result<()> {
    let mut sshc = backend.config_of(host)?;
    if let Some(run) = backend
        .forwards
        .lock()
        .unwrap()
        .remove(&key(host, kind, value))
    {
        run.task.abort();
    }
    let option = FORWARD_KEYS.iter().find(|(k, _)| *k == kind);
    let (_, option) = option.context("a forward is of kind L, R or D")?;
    ensure!(
        sshc.own_all(host, option).contains(&value),
        "{host} has no forward {kind} {value}"
    );
    sshc.remove(host, option, value);
    backend.save(&sshc)
}

/// Stops a forward if it runs and takes it out of the host's block.
#[tauri::command(async)]
pub fn forward_kill(
    host: String,
    kind: char,
    value: String,
    backend: State<'_, Backend>,
) -> Reply<()> {
    Ok(kill(&backend, &host, kind, &value)?)
}
