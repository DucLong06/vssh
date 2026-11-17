//! vssh desktop: a Tauri shell over `vssh-core`. This crate is the thin
//! layer between the window and the core: each command reads what the TUI
//! reads and answers in the core's own types, as JSON. It never names the
//! terminal app (`scripts/check-core-deps.sh` holds that line).
//!
//! The answers' shapes are declared again in `src/lib/api.ts`;
//! `tests/commands.rs` is where the two are held together.

mod files;
mod forwards;
mod hosts;
mod key_setup;
mod snippets;
mod vault;

use std::collections::HashMap;
use std::future::Future;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, ensure, Context, Result};
use serde::Serialize;
use tauri::State;
use tokio::sync::oneshot;
use vssh_core::config::{data_dir, ssh_config_path, Config, Lang};
use vssh_core::conn_err::{ConnError, Report};
use vssh_core::sftp::{self, Entry};
use vssh_core::ssh::{self, Target};
use vssh_core::stats::Peek;
use vssh_core::store::db::{Db, Snippet};
use vssh_core::store::ssh_config::{Forward, SshConfig};
use vssh_core::store::{self, Host};
use vssh_core::task::{self, Event};
use vssh_core::vault::Vault;
use zeroize::Zeroizing;

/// What a command answers: its value, or a failure the window can show.
type Reply<T> = Result<T, ConnError>;

/// An answer from the network, some time later.
pub type Answer<T> = Pin<Box<dyn Future<Output = Reply<T>> + Send>>;

/// A host's password from the vault, wiped when it is dropped.
pub type Password = Option<Zeroizing<String>>;

/// Hears how far a transfer is: bytes done, bytes in all.
pub type Progress = Box<dyn Fn(u64, u64) + Send + Sync>;

/// Called once a forward listens.
pub type Ready = Box<dyn FnOnce() + Send>;

/// A host that is logged in to: runs one command after another over the
/// one connection, which lasts as long as this does.
pub type Remote = Box<dyn Fn(String) -> Answer<String> + Send + Sync>;

/// The network, as far as the commands use it. The app's is [`Core`]; the
/// contract tests put one in its place that answers from memory. Dropping
/// an answer that has not come yet stops the work behind it.
pub trait Transport: Send + Sync + 'static {
    /// One round of metrics from a host of the ssh config.
    fn peek(&self, alias: String, target: Target, password: Password) -> Answer<Peek>;

    /// One SFTP command: what a `List` found, nothing for the rest.
    fn sftp(
        &self,
        target: Target,
        password: Password,
        cmd: sftp::Cmd,
        progress: Progress,
    ) -> Answer<Vec<Entry>>;

    /// One port forward, for as long as it holds: the answer is why it ended.
    fn forward(&self, target: Target, password: Password, fwd: Forward, ready: Ready)
        -> Answer<()>;

    /// Logs in to a host for commands. Each answers with what it printed,
    /// and with its exit status if that is not zero.
    fn connect(&self, target: Target, password: Password) -> Answer<Remote>;

    /// Logs in with this key file and nothing else: the proof that the key
    /// opens the host.
    fn key_login(&self, target: Target, key: PathBuf) -> Answer<()>;
}

/// The real network: `vssh_core::task`, on Tauri's own tokio runtime.
pub struct Core;

impl Transport for Core {
    fn peek(&self, alias: String, target: Target, password: Password) -> Answer<Peek> {
        Box::pin(async move {
            let (tx, mut rx) = task::channel();
            task::peek(alias, Ok(target), password, move |ev| drop(tx.send(ev)));
            match rx.recv().await {
                Some(Event::Peek(_, answer)) => answer,
                _ => Err(anyhow!("the peek ended without an answer").into()),
            }
        })
    }

    fn sftp(
        &self,
        target: Target,
        password: Password,
        cmd: sftp::Cmd,
        progress: Progress,
    ) -> Answer<Vec<Entry>> {
        Box::pin(async move {
            // shortcut: a connection for every command; keep one per host
            // if browsing over a slow link drags.
            let h = ssh::connect(&target, password.as_ref().map(|p| p.as_str()), None).await?;
            Ok(sftp::once(&h, cmd, progress).await?)
        })
    }

    fn forward(
        &self,
        target: Target,
        password: Password,
        fwd: Forward,
        ready: Ready,
    ) -> Answer<()> {
        Box::pin(async move {
            let password = password.as_ref().map(|p| p.as_str());
            Ok(ssh::forward::run(&target, password, fwd, ready).await?)
        })
    }

    fn connect(&self, target: Target, password: Password) -> Answer<Remote> {
        Box::pin(async move {
            let h = ssh::connect(&target, password.as_ref().map(|p| p.as_str()), None).await?;
            let h = Arc::new(h);
            let exec = move |command: String| -> Answer<String> {
                let h = h.clone();
                Box::pin(async move { Ok(ssh::exec(&h, &command).await?) })
            };
            Ok(Box::new(exec) as Remote)
        })
    }

    fn key_login(&self, target: Target, key: PathBuf) -> Answer<()> {
        Box::pin(async move { Ok(ssh::connect_key(&target, &key).await.map(drop)?) })
    }
}

/// What the commands work on: where the user's hosts are, and the network.
pub struct Backend {
    ssh_config: PathBuf,
    data_dir: PathBuf,
    net: Box<dyn Transport>,
    sessions: AtomicU64,
    /// Transfers under way, by the id the window gave them: how to stop each.
    transfers: Mutex<HashMap<String, oneshot::Sender<()>>>,
    /// Forwards that were started and have not ended, by host and line.
    forwards: Mutex<HashMap<String, forwards::Running>>,
    /// Numbers what runs in the background.
    jobs: AtomicU64,
    batch_max_parallel: usize,
    /// The vault while it is unlocked.
    vault: Mutex<Option<Vault>>,
}

impl Backend {
    pub fn new(ssh_config: PathBuf, data_dir: PathBuf, net: impl Transport) -> Self {
        Self {
            ssh_config,
            data_dir,
            net: Box::new(net),
            sessions: AtomicU64::new(0),
            transfers: Mutex::default(),
            forwards: Mutex::default(),
            jobs: AtomicU64::new(0),
            batch_max_parallel: Config::<()>::default().batch_max_parallel,
            vault: Mutex::default(),
        }
    }

    /// How many hosts of a batch are worked on at once (`batch_max_parallel`
    /// of the config file).
    pub fn batch_max_parallel(mut self, hosts: usize) -> Self {
        self.batch_max_parallel = hosts;
        self
    }

    /// Read on every call, so an edit made in the TUI or an editor shows up.
    fn ssh_config(&self) -> Result<SshConfig> {
        SshConfig::load(&self.ssh_config)
            .with_context(|| format!("cannot read {}", self.ssh_config.display()))
    }

    /// Where the database and the vault are, made on first use the way the
    /// TUI makes it.
    fn data_dir(&self) -> Result<&Path> {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.data_dir)?;
        Ok(&self.data_dir)
    }

    /// The TUI's database.
    fn db(&self) -> Result<Db> {
        Ok(Db::open(&self.data_dir()?.join("vssh.db"))?)
    }

    fn hosts(&self) -> Result<Vec<Host>> {
        let sshc = self.ssh_config()?;
        let db = self.db()?;
        db.import(&sshc.hosts())?;
        Ok(store::hosts(&sshc, &db.metas()?))
    }

    /// The config, for work on one of its hosts. Only those: the window
    /// cannot make the app act on a name it made up.
    fn config_of(&self, alias: &str) -> Result<SshConfig> {
        let sshc = self.ssh_config()?;
        let known = sshc.hosts().iter().any(|host| host == alias);
        ensure!(known, "no host named {alias:?}");
        Ok(sshc)
    }

    fn save(&self, sshc: &SshConfig) -> Result<()> {
        sshc.save(&self.ssh_config)
            .with_context(|| format!("cannot write {}", self.ssh_config.display()))
    }

    /// A host of the config, resolved the way `ssh alias` would.
    fn target(&self, alias: &str) -> Result<Target> {
        Target::from_config(&self.config_of(alias)?, alias)
    }

    /// How to log in to a host of the config: where it is, and its password
    /// if the vault has one.
    fn login(&self, alias: &str) -> Result<(Target, Password)> {
        let target = self.target(alias)?;
        let vault = self.vault.lock().unwrap();
        let password = vault.as_ref().and_then(|vault| vault.get(alias));
        Ok((target, password.map(|p| Zeroizing::new(p.to_string()))))
    }
}

#[tauri::command(async)]
fn list_hosts(backend: State<'_, Backend>) -> Reply<Vec<Host>> {
    Ok(backend.hosts()?)
}

#[tauri::command]
async fn get_host_stats(alias: String, backend: State<'_, Backend>) -> Reply<Peek> {
    let (target, password) = backend.login(&alias)?;
    backend.net.peek(alias, target, password).await
}

/// Opens a session to a typed `[user@]host[:port]` and returns its id.
/// Nothing is saved, as in the TUI.
#[tauri::command]
fn quick_connect(host: String, backend: State<'_, Backend>) -> Reply<u64> {
    Target::parse(&host)?;
    // shortcut: no connection is made, the id is all there is to a session;
    // D3 opens the PTY here, through `Transport`.
    Ok(backend.sessions.fetch_add(1, Ordering::Relaxed) + 1)
}

// `snippets_list` under the name the window first knew it by.
#[tauri::command(async)]
fn list_snippets(backend: State<'_, Backend>) -> Reply<Vec<Snippet>> {
    snippets::snippets_list(backend)
}

/// A failure in the words of one language, with its class's label.
#[derive(Serialize)]
struct Shown {
    label: &'static str,
    #[serde(flatten)]
    report: Report,
}

/// Puts an error a command answered with into words. The window keeps the
/// error itself, so it can ask again when the language changes.
#[tauri::command]
fn resolve_conn_error(error: ConnError, lang: Lang) -> Shown {
    let report = error.report(lang);
    Shown {
        label: report.class.label(lang),
        report,
    }
}

/// The app without a window: its state and its commands, on any runtime.
pub fn builder<R: tauri::Runtime>(backend: Backend) -> tauri::Builder<R> {
    tauri::Builder::new()
        .manage(backend)
        .invoke_handler(tauri::generate_handler![
            list_hosts,
            get_host_stats,
            quick_connect,
            list_snippets,
            resolve_conn_error,
            files::sftp_list,
            files::sftp_mkdir,
            files::sftp_rm,
            files::sftp_rename,
            files::sftp_upload,
            files::sftp_download,
            files::sftp_cancel,
            forwards::forwards_list,
            forwards::forward_add,
            forwards::forward_toggle,
            forwards::forward_kill,
            snippets::snippets_list,
            snippets::snippets_create,
            snippets::snippets_update,
            snippets::snippets_delete,
            snippets::snippet_run,
            vault::vault_state,
            vault::vault_unlock,
            vault::vault_pass_change,
            vault::vault_export,
            hosts::hosts_add,
            hosts::hosts_update,
            hosts::hosts_delete,
            hosts::hosts_import_ssh_config,
            hosts::host_quick_save,
            key_setup::ssh_key_setup
        ])
}

/// Opens the window over the user's own hosts and runs until it is closed.
pub fn run() {
    // A config file that cannot be read is the TUI's to complain about.
    let config: Config = Config::load().unwrap_or_default();
    let backend = Backend::new(ssh_config_path(), data_dir(), Core)
        .batch_max_parallel(config.batch_max_parallel);
    builder::<tauri::Wry>(backend)
        .run(tauri::generate_context!())
        .expect("vssh could not start its window");
}
