//! SFTP worker: owns one SFTP session and executes commands from the UI in
//! order, reporting listings and transfer progress back.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use std::{fs, io};

use anyhow::Result;
use russh::client::Handle;
use russh_sftp::client::SftpSession;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::ssh::Client;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// Last change, Unix seconds; 0 = not known.
    pub mtime: u64,
}

#[derive(Debug)]
pub enum Cmd {
    /// Change to and list a directory.
    List(String),
    /// Where a path leads, in full: the answer is one entry, named by it.
    Realpath(String),
    Upload(PathBuf, String),
    Download(String, PathBuf),
    Mkdir(String),
    Rename(String, String),
    /// Path, is a directory.
    Delete(String, bool),
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Reply {
    /// Canonical remote directory and its contents.
    Listing(String, Vec<Entry>),
    /// Bytes done, bytes total.
    Progress(u64, u64),
    Done(String),
    Error(String),
}

fn sorted(mut entries: Vec<Entry>) -> Vec<Entry> {
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    entries
}

pub fn list_local(dir: &Path) -> io::Result<Vec<Entry>> {
    let entries = fs::read_dir(dir)?.filter_map(|e| {
        let e = e.ok()?;
        // Follows symlinks, so a link to a directory can be entered.
        let meta = fs::metadata(e.path()).ok()?;
        let changed = meta.modified().ok();
        let changed = changed.and_then(|at| at.duration_since(UNIX_EPOCH).ok());
        Some(Entry {
            name: e.file_name().to_string_lossy().into_owned(),
            is_dir: meta.is_dir(),
            size: meta.len(),
            mtime: changed.map_or(0, |d| d.as_secs()),
        })
    });
    Ok(sorted(entries.collect()))
}

// shortcut: one request at a time, pipeline the writes if throughput on
// high-latency links matters.
async fn copy(
    mut from: impl AsyncRead + Unpin,
    mut to: impl AsyncWrite + Unpin,
    total: u64,
    progress: &impl Fn(u64, u64),
) -> Result<()> {
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0;
    loop {
        let n = from.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        to.write_all(&buf[..n]).await?;
        done += n as u64;
        progress(done, total);
    }
    // Flushes and closes; a failure here means the tail was not written.
    to.shutdown().await?;
    Ok(())
}

async fn open(h: &Handle<Client>) -> Result<SftpSession> {
    let channel = h.channel_open_session().await?;
    channel.request_subsystem(true, "sftp").await?;
    Ok(SftpSession::new(channel.into_stream()).await?)
}

async fn read_dir(sftp: &SftpSession, dir: &str) -> Result<Vec<Entry>> {
    let entries = sftp.read_dir(dir).await?.map(|e| {
        let meta = e.metadata();
        Entry {
            name: e.file_name(),
            is_dir: meta.is_dir(),
            size: meta.len(),
            mtime: meta.mtime.unwrap_or(0).into(),
        }
    });
    Ok(sorted(entries.collect()))
}

/// Carries out a command that changes something and returns the words for
/// it. `List` changes nothing.
async fn apply(sftp: &SftpSession, cmd: Cmd, progress: &impl Fn(u64, u64)) -> Result<&'static str> {
    Ok(match cmd {
        Cmd::List(_) | Cmd::Realpath(_) => "",
        Cmd::Upload(local, remote) => {
            let from = tokio::fs::File::open(local).await?;
            let total = from.metadata().await?.len();
            copy(from, sftp.create(remote).await?, total, progress).await?;
            "upload complete"
        }
        Cmd::Download(remote, local) => {
            let from = sftp.open(remote).await?;
            let total = from.metadata().await?.len();
            let to = tokio::fs::File::create(local).await?;
            copy(from, to, total, progress).await?;
            "download complete"
        }
        Cmd::Mkdir(path) => {
            sftp.create_dir(path).await?;
            "directory created"
        }
        Cmd::Rename(from, to) => {
            sftp.rename(from, to).await?;
            "renamed"
        }
        Cmd::Delete(path, true) => {
            sftp.remove_dir(path).await?;
            "deleted"
        }
        Cmd::Delete(path, false) => {
            sftp.remove_file(path).await?;
            "deleted"
        }
    })
}

/// One command for a frontend that asks and waits for the answer: what a
/// `List` found, nothing for the rest. Dropping the future stops a transfer
/// where it is.
pub async fn once(h: &Handle<Client>, cmd: Cmd, progress: impl Fn(u64, u64)) -> Result<Vec<Entry>> {
    let sftp = open(h).await?;
    match cmd {
        Cmd::List(path) => read_dir(&sftp, &sftp.canonicalize(path).await?).await,
        Cmd::Realpath(path) => Ok(vec![Entry {
            name: sftp.canonicalize(path).await?,
            is_dir: true,
            size: 0,
            mtime: 0,
        }]),
        change => apply(&sftp, change, &progress).await.map(|_| vec![]),
    }
}

pub async fn worker(
    h: Handle<Client>,
    mut rx: UnboundedReceiver<Cmd>,
    reply: impl Fn(Reply),
) -> Result<()> {
    let sftp = open(&h).await?;
    let mut cwd = sftp.canonicalize(".").await?;
    let mut cmd = Cmd::List(cwd.clone());
    let progress = |done, total| reply(Reply::Progress(done, total));
    loop {
        let res = async {
            let (dir, msg) = match cmd {
                Cmd::List(path) => (sftp.canonicalize(path).await?, ""),
                change => (cwd.clone(), apply(&sftp, change, &progress).await?),
            };
            anyhow::Ok((read_dir(&sftp, &dir).await?, dir, msg))
        }
        .await;
        match res {
            Ok((entries, dir, msg)) => {
                cwd = dir;
                reply(Reply::Listing(cwd.clone(), entries));
                if !msg.is_empty() {
                    reply(Reply::Done(msg.to_string()));
                }
            }
            Err(e) => reply(Reply::Error(e.to_string())),
        }
        match rx.recv().await {
            Some(next) => cmd = next,
            None => return Ok(()),
        }
    }
}
