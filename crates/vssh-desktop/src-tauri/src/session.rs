//! Terminal sessions: a shell on a PTY per tab, through the core.
//!
//! A session is opened first and started second: `session_open` only gives
//! it an id, and the connection is made by `session_start`, which the
//! terminal calls once it listens and knows its size. So no output is lost
//! on the way to the window, and the PTY is the right size from its first
//! line. Starting again is how a tab reconnects.
//!
//! Output and state go to the window as events (`pty-data:<id>`,
//! `pty-state:<id>`); typing and resizing come in as commands, because an
//! event from the window is handled on any thread of the runtime and two
//! keys could overtake each other, where these commands run in order.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use vssh_core::conn_err::ConnError;
use vssh_core::ssh::{SessionCmd, Target, Via};
use vssh_core::task::{self, Sender, Shell};
use zeroize::Zeroizing;

use crate::{Backend, Password, Reply};

/// A tab's session: where it goes, the host of the config it is (none for
/// a quick connect), and its shell while one runs.
struct Session {
    target: Target,
    alias: Option<String>,
    keys: Option<Sender<SessionCmd>>,
    shell: Option<JoinHandle<()>>,
}

impl Drop for Session {
    /// Ends the shell: its connection is dropped where it is.
    fn drop(&mut self) {
        if let Some(shell) = self.shell.take() {
            shell.abort();
        }
    }
}

/// The open sessions, by id. Ids are never used twice.
#[derive(Default)]
pub struct Sessions {
    last: u64,
    open: HashMap<u64, Session>,
}

impl Sessions {
    fn add(&mut self, target: Target, alias: Option<String>) -> u64 {
        self.last += 1;
        let session = Session {
            target,
            alias,
            keys: None,
            shell: None,
        };
        self.open.insert(self.last, session);
        self.last
    }

    fn get(&mut self, id: u64) -> Result<&mut Session> {
        self.open
            .get_mut(&id)
            .ok_or_else(|| anyhow!("no session {id}"))
    }
}

/// What `pty-state:<id>` carries.
#[derive(Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum PtyState {
    /// Connected and logged in.
    Up,
    /// The command in the foreground; `null` at the prompt.
    Busy { command: Option<String> },
    /// The shell ended: cleanly, or with what ended it.
    Closed { error: Option<ConnError> },
}

/// Opens a session to a host of the config and returns its id.
#[tauri::command(async)]
pub fn session_open(alias: String, backend: State<'_, Backend>) -> Reply<u64> {
    let target = backend.target(&alias)?;
    Ok(backend.sessions.lock().unwrap().add(target, Some(alias)))
}

/// Opens a session to a typed `[user@]host[:port]` and returns its id.
/// Nothing is saved, as in the TUI.
#[tauri::command]
pub fn quick_connect(host: String, backend: State<'_, Backend>) -> Reply<u64> {
    let target = Target::parse(&host)?;
    Ok(backend.sessions.lock().unwrap().add(target, None))
}

/// What a start keeps in the vault once its login went through: the host
/// and the password typed for it.
type Keep = Option<(String, Zeroizing<String>)>;

/// The password a start logs in with: the one typed for it, else the
/// vault's for the host. With `save`, the typed one is also what to keep:
/// for a host of the config, in a vault unlocked now. Never logged, never
/// written anywhere but the vault, and only by [`keep`].
fn password(
    backend: &Backend,
    alias: Option<&str>,
    typed: Option<String>,
    save: bool,
) -> Result<(Password, Keep)> {
    let Some(typed) = typed.map(Zeroizing::new).filter(|p| !p.is_empty()) else {
        return Ok((alias.and_then(|alias| backend.password(alias)), None));
    };
    if !save {
        return Ok((Some(typed), None));
    }
    let alias =
        alias.ok_or_else(|| anyhow!("a quick connect has no host to keep a password for"))?;
    if backend.vault.lock().unwrap().is_none() {
        return Err(anyhow!("the vault is locked"));
    }
    Ok((Some(typed.clone()), Some((alias.to_string(), typed))))
}

/// Puts a typed password into the vault, once the login with it went
/// through: a wrong one never replaces the one there.
fn keep<R: Runtime>(app: &AppHandle<R>, (alias, password): (String, Zeroizing<String>)) {
    let backend = app.state::<Backend>();
    let mut vault = backend.vault.lock().unwrap();
    // shortcut: a vault locked since the start, or a write that fails, keeps nothing and says nothing; tell the window once someone misses it.
    if let Some(vault) = vault.as_mut() {
        let _ = vault.set(&alias, &mut password.to_string());
    }
}

/// Connects the session and runs its shell on a PTY of this size. A shell
/// it already had is ended first. `password` is one the person typed after
/// a refused login; `save` keeps it in the (unlocked) vault for the host,
/// once it logged in.
#[tauri::command]
pub fn session_start<R: Runtime>(
    id: u64,
    cols: u16,
    rows: u16,
    password: Option<String>,
    save: Option<bool>,
    app: AppHandle<R>,
    backend: State<'_, Backend>,
) -> Reply<()> {
    let mut sessions = backend.sessions.lock().unwrap();
    let session = sessions.get(id)?;
    let (password, keep) = self::password(
        &backend,
        session.alias.as_deref(),
        password,
        save == Some(true),
    )?;
    if let Some(old) = session.shell.take() {
        old.abort();
    }
    let (keys, typed) = task::channel();
    let state = format!("pty-state:{id}");
    let (window, data, said) = (app.clone(), format!("pty-data:{id}"), state.clone());
    // A window that is gone hears nothing, which is all there is to do about it.
    let keep = Mutex::new(keep);
    let report = Arc::new(move |ev| {
        let _ = match ev {
            Shell::Up => {
                if let Some(typed) = keep.lock().unwrap().take() {
                    self::keep(&window, typed);
                }
                window.emit(&said, PtyState::Up)
            }
            // shortcut: bytes go as a JSON array of numbers, some four
            // characters each; use a `tauri::ipc::Channel` with a raw body
            // if a terminal ever needs to keep up with `cat` of a large file.
            Shell::Data(bytes) => window.emit(&data, bytes),
            Shell::Busy(command) => window.emit(&said, PtyState::Busy { command }),
        };
    });
    let size = (cols, rows);
    let running = backend
        .net
        .shell(session.target.clone(), password, size, typed, report);
    session.keys = Some(keys);
    session.shell = Some(tauri::async_runtime::spawn(async move {
        let error = running.await.err();
        let _ = app.emit(&state, PtyState::Closed { error });
    }));
    Ok(())
}

/// Sends a command to the session's shell. One that has no shell, or whose
/// shell just ended, drops it: keys typed at a dead terminal go nowhere.
fn tell(backend: &Backend, id: u64, cmd: SessionCmd) -> Reply<()> {
    let mut sessions = backend.sessions.lock().unwrap();
    if let Some(keys) = &sessions.get(id)?.keys {
        let _ = keys.send(cmd);
    }
    Ok(())
}

/// Types into the session.
#[tauri::command]
pub fn session_write(id: u64, data: String, backend: State<'_, Backend>) -> Reply<()> {
    tell(&backend, id, SessionCmd::Data(data.into_bytes()))
}

#[tauri::command]
pub fn session_resize(id: u64, cols: u16, rows: u16, backend: State<'_, Backend>) -> Reply<()> {
    tell(&backend, id, SessionCmd::Resize(cols, rows))
}

/// Ends the session and forgets it.
#[tauri::command]
pub fn session_close(id: u64, backend: State<'_, Backend>) -> Reply<()> {
    let mut sessions = backend.sessions.lock().unwrap();
    sessions.get(id)?;
    sessions.open.remove(&id);
    Ok(())
}

/// A person's yes to the host key that stopped this session (`host_key`),
/// good for its next start: once, or `forever`, which puts the key on
/// record. Only for a host the session goes to or through.
#[tauri::command]
pub fn session_trust(
    id: u64,
    host: String,
    fingerprint: String,
    forever: bool,
    backend: State<'_, Backend>,
) -> Reply<()> {
    let mut sessions = backend.sessions.lock().unwrap();
    let target = &sessions.get(id)?.target;
    let hops = match &target.via {
        Via::Jump(hops) => hops.as_slice(),
        _ => &[],
    };
    let on_road = hops.iter().chain([target]).find(|t| t.host == host);
    let on_road = on_road.ok_or_else(|| anyhow!("session {id} does not go through {host:?}"))?;
    backend.net.trust(on_road, &fingerprint, forever);
    Ok(())
}
