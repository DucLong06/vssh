//! SFTP worker: owns one SFTP session and executes commands from the UI in
//! order, reporting listings and transfer progress back.

use std::path::{Path, PathBuf};
use std::{fs, io};

use anyhow::Result;
use russh::client::Handle;
use russh_sftp::client::SftpSession;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::ssh::Client;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

pub enum Cmd {
    /// Change to and list a directory.
    List(String),
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
        Some(Entry {
            name: e.file_name().to_string_lossy().into_owned(),
            is_dir: meta.is_dir(),
            size: meta.len(),
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
    reply: &impl Fn(Reply),
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
        reply(Reply::Progress(done, total));
    }
    // Flushes and closes; a failure here means the tail was not written.
    to.shutdown().await?;
    Ok(())
}

pub async fn worker(
    h: Handle<Client>,
    mut rx: UnboundedReceiver<Cmd>,
    reply: impl Fn(Reply),
) -> Result<()> {
    let channel = h.channel_open_session().await?;
    channel.request_subsystem(true, "sftp").await?;
    let sftp = SftpSession::new(channel.into_stream()).await?;
    let mut cwd = sftp.canonicalize(".").await?;
    let mut cmd = Cmd::List(cwd.clone());
    loop {
        let res = async {
            let (dir, msg) = match cmd {
                Cmd::List(path) => (sftp.canonicalize(path).await?, ""),
                Cmd::Upload(local, remote) => {
                    let from = tokio::fs::File::open(local).await?;
                    let total = from.metadata().await?.len();
                    copy(from, sftp.create(remote).await?, total, &reply).await?;
                    (cwd.clone(), "upload complete")
                }
                Cmd::Download(remote, local) => {
                    let from = sftp.open(remote).await?;
                    let total = from.metadata().await?.len();
                    let to = tokio::fs::File::create(local).await?;
                    copy(from, to, total, &reply).await?;
                    (cwd.clone(), "download complete")
                }
                Cmd::Mkdir(path) => {
                    sftp.create_dir(path).await?;
                    (cwd.clone(), "directory created")
                }
                Cmd::Rename(from, to) => {
                    sftp.rename(from, to).await?;
                    (cwd.clone(), "renamed")
                }
                Cmd::Delete(path, true) => {
                    sftp.remove_dir(path).await?;
                    (cwd.clone(), "deleted")
                }
                Cmd::Delete(path, false) => {
                    sftp.remove_file(path).await?;
                    (cwd.clone(), "deleted")
                }
            };
            let entries = sftp.read_dir(dir.as_str()).await?.map(|e| {
                let meta = e.metadata();
                Entry {
                    name: e.file_name(),
                    is_dir: meta.is_dir(),
                    size: meta.len(),
                }
            });
            anyhow::Ok((dir, sorted(entries.collect()), msg))
        }
        .await;
        match res {
            Ok((dir, entries, msg)) => {
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
