//! SFTP for the window (D4). A listing or a small change is asked and
//! answered; a transfer answers when it is over and says how far it is on
//! the event `sftp-progress:<id>` meanwhile, under an id the window picks
//! (so it can listen before it asks) and can cancel it by. The `local_*`
//! commands are the other pane: the same on this machine, which the window
//! cannot read on its own.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fs, io};

use anyhow::{anyhow, ensure, Context, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime, State};
use tokio::sync::oneshot;
use vssh_core::config::home;
use vssh_core::sftp::{list_local, Cmd, Entry};

use crate::{Backend, Reply};

async fn run(backend: &Backend, host: &str, cmd: Cmd) -> Reply<Vec<Entry>> {
    let (target, password) = backend.login(host)?;
    let silent = Box::new(|_, _| {});
    backend.net.sftp(target, password, cmd, silent).await
}

/// The entries of a remote directory, directories first. An empty path is
/// the directory the server starts in.
#[tauri::command]
pub async fn sftp_list(
    host: String,
    path: String,
    backend: State<'_, Backend>,
) -> Reply<Vec<Entry>> {
    let path = if path.is_empty() { ".".into() } else { path };
    run(&backend, &host, Cmd::List(path)).await
}

/// Where a remote path leads, in full. An empty path is the directory the
/// server starts in: this is how the window learns where that is.
#[tauri::command]
pub async fn sftp_realpath(
    host: String,
    path: String,
    backend: State<'_, Backend>,
) -> Reply<String> {
    let path = if path.is_empty() { ".".into() } else { path };
    let found = run(&backend, &host, Cmd::Realpath(path)).await?;
    Ok(found.into_iter().next().map(|e| e.name).unwrap_or_default())
}

/// A directory of this machine: where it is, in full, and what is in it.
#[derive(Serialize)]
pub struct Listing {
    path: String,
    entries: Vec<Entry>,
}

/// An io error, naming the path it is about.
fn at<T>(path: &Path, res: io::Result<T>) -> Result<T> {
    res.with_context(|| path.display().to_string())
}

/// The entries of a local directory, directories first. An empty path is
/// the user's home.
#[tauri::command(async)]
pub fn local_list(path: String) -> Reply<Listing> {
    let asked = if path.is_empty() { home() } else { path.into() };
    let dir = at(&asked, asked.canonicalize())?;
    Ok(Listing {
        entries: at(&dir, list_local(&dir))?,
        path: dir.to_string_lossy().into_owned(),
    })
}

#[tauri::command(async)]
pub fn local_mkdir(path: PathBuf) -> Reply<()> {
    Ok(at(&path, fs::create_dir(&path))?)
}

/// Removes a file, or a directory that is empty.
#[tauri::command(async)]
pub fn local_rm(path: PathBuf, is_dir: bool) -> Reply<()> {
    let removed = if is_dir {
        fs::remove_dir(&path)
    } else {
        fs::remove_file(&path)
    };
    Ok(at(&path, removed)?)
}

#[tauri::command(async)]
pub fn local_rename(from: PathBuf, to: PathBuf) -> Reply<()> {
    Ok(at(&from, fs::rename(&from, &to))?)
}

#[tauri::command]
pub async fn sftp_mkdir(host: String, path: String, backend: State<'_, Backend>) -> Reply<()> {
    run(&backend, &host, Cmd::Mkdir(path)).await.map(drop)
}

/// Removes a file, or a directory that is empty.
#[tauri::command]
pub async fn sftp_rm(
    host: String,
    path: String,
    is_dir: bool,
    backend: State<'_, Backend>,
) -> Reply<()> {
    run(&backend, &host, Cmd::Delete(path, is_dir))
        .await
        .map(drop)
}

#[tauri::command]
pub async fn sftp_rename(
    host: String,
    from: String,
    to: String,
    backend: State<'_, Backend>,
) -> Reply<()> {
    run(&backend, &host, Cmd::Rename(from, to)).await.map(drop)
}

/// What `sftp-progress:<id>` carries.
#[derive(Clone, Serialize)]
struct Moved {
    done: u64,
    total: u64,
}

/// Takes the id for a transfer and returns what tells when it is cancelled.
fn begin(backend: &Backend, id: &str) -> Result<oneshot::Receiver<()>> {
    // The id becomes part of an event name, which allows little else.
    let plain = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    ensure!(
        !id.is_empty() && id.chars().all(plain),
        "a transfer id is made of letters, digits, - and _"
    );
    let mut transfers = backend.transfers.lock().unwrap();
    ensure!(
        !transfers.contains_key(id),
        "transfer {id} is already running"
    );
    let (stop, stopped) = oneshot::channel();
    transfers.insert(id.to_string(), stop);
    Ok(stopped)
}

/// Runs one transfer until it is over, has failed, or is cancelled.
async fn transfer<R: Runtime>(
    app: AppHandle<R>,
    backend: &Backend,
    host: &str,
    id: String,
    cmd: Cmd,
) -> Reply<()> {
    let (target, password) = backend.login(host)?;
    let stopped = begin(backend, &id)?;
    let event = format!("sftp-progress:{id}");
    // A chunk is 64 KiB: the window hears of each new percent, not of each chunk.
    let percent = AtomicU64::new(u64::MAX);
    let progress = move |done: u64, total: u64| {
        let now = done * 100 / total.max(1);
        if percent.swap(now, Ordering::Relaxed) != now {
            let _ = app.emit(&event, Moved { done, total });
        }
    };
    let moving = backend.net.sftp(target, password, cmd, Box::new(progress));
    let res = tokio::select! {
        res = moving => res.map(drop),
        // Dropping `moving` is what stops it.
        _ = stopped => Err(anyhow!("transfer cancelled").into()),
    };
    backend.transfers.lock().unwrap().remove(&id);
    res
}

/// Copies a local file to `remote`, the full path of the file to write.
#[tauri::command]
pub async fn sftp_upload<R: Runtime>(
    app: AppHandle<R>,
    host: String,
    local: PathBuf,
    remote: String,
    id: String,
    backend: State<'_, Backend>,
) -> Reply<()> {
    transfer(app, &backend, &host, id, Cmd::Upload(local, remote)).await
}

/// Copies a remote file to `local`, the full path of the file to write.
#[tauri::command]
pub async fn sftp_download<R: Runtime>(
    app: AppHandle<R>,
    host: String,
    remote: String,
    local: PathBuf,
    id: String,
    backend: State<'_, Backend>,
) -> Reply<()> {
    transfer(app, &backend, &host, id, Cmd::Download(remote, local)).await
}

/// Stops a transfer where it is; what was written so far stays. False if
/// no transfer has that id (any more).
#[tauri::command]
pub fn sftp_cancel(id: String, backend: State<'_, Backend>) -> bool {
    let stop = backend.transfers.lock().unwrap().remove(&id);
    stop.is_some_and(|stop| stop.send(()).is_ok())
}
