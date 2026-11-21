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

/// A path of this machine as the window has it: joined with `/`, from one
/// root, on every system. On Windows `C:\Users\me` is `/C:/Users/me` there,
/// and `/` is the list of the drives.
#[cfg(windows)]
fn shown(path: &Path) -> String {
    use std::path::{Component, Prefix};

    let mut out = String::new();
    for part in path.components() {
        match part {
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                    out.extend(['/', letter as char, ':']);
                }
                // shortcut: a network share is shown as Windows writes it, and the pane cannot climb out of it; give `\\server\share` a place under `/` once someone keeps files on one.
                _ => return path.display().to_string(),
            },
            Component::RootDir => {}
            part => {
                out.push('/');
                out.push_str(&part.as_os_str().to_string_lossy());
            }
        }
    }
    out
}

#[cfg(not(windows))]
fn shown(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// `/C:/Users/me` as Windows reads it: `C:/Users/me`. A drive alone gets
/// its `/`: a bare `C:` is wherever that drive was last left.
fn drive(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let lettered = bytes.len() >= 3
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
        && matches!(bytes.get(3), None | Some(b'/'));
    let bare = if bytes.len() == 3 { "/" } else { "" };
    lettered.then(|| format!("{}{bare}", &path[1..]))
}

/// The path of this machine that one of the window's stands for. A path
/// that came from the system itself (a file dropped on the window) is itself.
fn real(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(drive) {
        Some(native) if cfg!(windows) => native.into(),
        _ => path,
    }
}

/// Every drive there is, as the directories of `/`.
#[cfg(windows)]
fn drives() -> Vec<Entry> {
    let there = |name: &String| Path::new(&format!("{name}/")).exists();
    let drives = ('A'..='Z').map(|letter| format!("{letter}:")).filter(there);
    let entry = |name| Entry {
        name,
        is_dir: true,
        size: 0,
        mtime: 0,
    };
    drives.map(entry).collect()
}

/// The entries of a local directory, directories first. An empty path is
/// the user's home.
#[tauri::command(async)]
pub fn local_list(path: String) -> Reply<Listing> {
    #[cfg(windows)]
    if path == "/" {
        let entries = drives();
        return Ok(Listing { path, entries });
    }
    let asked = if path.is_empty() {
        home()
    } else {
        real(path.into())
    };
    let dir = at(&asked, asked.canonicalize())?;
    Ok(Listing {
        entries: at(&dir, list_local(&dir))?,
        path: shown(&dir),
    })
}

#[tauri::command(async)]
pub fn local_mkdir(path: PathBuf) -> Reply<()> {
    let path = real(path);
    Ok(at(&path, fs::create_dir(&path))?)
}

/// Removes a file, or a directory that is empty.
#[tauri::command(async)]
pub fn local_rm(path: PathBuf, is_dir: bool) -> Reply<()> {
    let path = real(path);
    let removed = if is_dir {
        fs::remove_dir(&path)
    } else {
        fs::remove_file(&path)
    };
    Ok(at(&path, removed)?)
}

#[tauri::command(async)]
pub fn local_rename(from: PathBuf, to: PathBuf) -> Reply<()> {
    let (from, to) = (real(from), real(to));
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
    transfer(app, &backend, &host, id, Cmd::Upload(real(local), remote)).await
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
    let cmd = Cmd::Download(remote, real(local));
    transfer(app, &backend, &host, id, cmd).await
}

/// Stops a transfer where it is; what was written so far stays. False if
/// no transfer has that id (any more).
#[tauri::command]
pub fn sftp_cancel(id: String, backend: State<'_, Backend>) -> bool {
    let stop = backend.transfers.lock().unwrap().remove(&id);
    stop.is_some_and(|stop| stop.send(()).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_windows_drive_is_a_directory_of_the_windows_one_root() {
        assert_eq!(drive("/C:").as_deref(), Some("C:/"));
        assert_eq!(drive("/c:/Users/me").as_deref(), Some("c:/Users/me"));
        assert_eq!(drive("/D:/a b/c.txt").as_deref(), Some("D:/a b/c.txt"));
        // The root, a Unix path, a name that only starts like a drive, a dropped file's own path.
        for other in [
            "/",
            "",
            "/home/me",
            "/C:x",
            "/CD:/x",
            "C:\\Users\\me",
            "/é:",
        ] {
            assert_eq!(drive(other), None, "{other}");
        }
        // Only Windows reads it so: anywhere else `/C:` is a directory called `C:`.
        let real = |path: &str| real(path.into()).to_string_lossy().into_owned();
        assert_eq!(real("/C:/x"), if cfg!(windows) { "C:/x" } else { "/C:/x" });
        assert_eq!(real("C:\\x\\y.txt"), "C:\\x\\y.txt");
    }

    #[cfg(windows)]
    #[test]
    fn a_windows_path_is_shown_from_one_root_and_read_back() {
        assert_eq!(shown(Path::new(r"C:\Users\me")), "/C:/Users/me");
        assert_eq!(shown(Path::new(r"\\?\C:\Users\me")), "/C:/Users/me");
        assert_eq!(shown(Path::new(r"\\?\D:\")), "/D:");
        // What the pane shows, it can ask for again.
        let home = home().canonicalize().unwrap();
        assert_eq!(real(shown(&home).into()).canonicalize().unwrap(), home);
        assert!(drives()
            .iter()
            .any(|drive| drive.is_dir && drive.name.len() == 2));
        // A share is not climbed out of: shown as it is.
        assert_eq!(shown(Path::new(r"\\srv\share\x")), r"\\srv\share\x");
    }
}
