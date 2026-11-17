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
use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Runtime, State};
use vssh_core::conn_err::ConnError;
use vssh_core::ssh::{SessionCmd, Target, Via};
use vssh_core::task::{self, Sender, Shell};

use crate::{Backend, Reply};

/// A tab's session: where it goes, and its shell while one runs.
struct Session {
    target: Target,
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
    fn add(&mut self, target: Target) -> u64 {
        self.last += 1;
        let session = Session {
            target,
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
    Ok(backend.sessions.lock().unwrap().add(target))
}

/// Opens a session to a typed `[user@]host[:port]` and returns its id.
/// Nothing is saved, as in the TUI.
#[tauri::command]
pub fn quick_connect(host: String, backend: State<'_, Backend>) -> Reply<u64> {
    let target = Target::parse(&host)?;
    Ok(backend.sessions.lock().unwrap().add(target))
}

/// Connects the session and runs its shell on a PTY of this size. A shell
/// it already had is ended first.
#[tauri::command]
pub fn session_start<R: Runtime>(
    id: u64,
    cols: u16,
    rows: u16,
    app: AppHandle<R>,
    backend: State<'_, Backend>,
) -> Reply<()> {
    let mut sessions = backend.sessions.lock().unwrap();
    let session = sessions.get(id)?;
    if let Some(old) = session.shell.take() {
        old.abort();
    }
    let (keys, typed) = task::channel();
    let state = format!("pty-state:{id}");
    let (window, data, said) = (app.clone(), format!("pty-data:{id}"), state.clone());
    // A window that is gone hears nothing, which is all there is to do about it.
    let report = Arc::new(move |ev| {
        let _ = match ev {
            Shell::Up => window.emit(&said, PtyState::Up),
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
        .shell(session.target.clone(), size, typed, report);
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
