//! vssh desktop: a Tauri shell over `vssh-core`. This crate is the thin
//! layer between the window and the core: each command reads what the TUI
//! reads and answers in the core's own types, as JSON. It never names the
//! terminal app (`scripts/check-core-deps.sh` holds that line).
//!
//! The answers' shapes are declared again in `src/lib/api.ts`;
//! `tests/commands.rs` is where the two are held together.

use std::future::Future;
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{anyhow, ensure, Context, Result};
use serde::Serialize;
use tauri::State;
use vssh_core::config::{data_dir, ssh_config_path, Lang};
use vssh_core::conn_err::{ConnError, Report};
use vssh_core::ssh::Target;
use vssh_core::stats::Peek;
use vssh_core::store::db::{Db, Snippet};
use vssh_core::store::ssh_config::SshConfig;
use vssh_core::store::{self, Host};
use vssh_core::task::{self, Event};

/// What a command answers: its value, or a failure the window can show.
type Reply<T> = Result<T, ConnError>;

/// An answer from the network, some time later.
pub type Answer<T> = Pin<Box<dyn Future<Output = Reply<T>> + Send>>;

/// The network, as far as the commands use it. The app's is [`Core`]; the
/// contract tests put one in its place that answers from memory.
pub trait Transport: Send + Sync + 'static {
    /// One round of metrics from a host of the ssh config.
    fn peek(&self, alias: String, target: Target) -> Answer<Peek>;
}

/// The real network: `vssh_core::task`, on Tauri's own tokio runtime.
pub struct Core;

impl Transport for Core {
    fn peek(&self, alias: String, target: Target) -> Answer<Peek> {
        Box::pin(async move {
            let (tx, mut rx) = task::channel();
            // Keys and the agent only: the vault has no window yet (D6).
            task::peek(alias, Ok(target), None, move |ev| drop(tx.send(ev)));
            match rx.recv().await {
                Some(Event::Peek(_, answer)) => answer,
                _ => Err(anyhow!("the peek ended without an answer").into()),
            }
        })
    }
}

/// What the commands work on: where the user's hosts are, and the network.
pub struct Backend {
    ssh_config: PathBuf,
    data_dir: PathBuf,
    net: Box<dyn Transport>,
    sessions: AtomicU64,
}

impl Backend {
    pub fn new(ssh_config: PathBuf, data_dir: PathBuf, net: impl Transport) -> Self {
        Self {
            ssh_config,
            data_dir,
            net: Box::new(net),
            sessions: AtomicU64::new(0),
        }
    }

    /// Read on every call, so an edit made in the TUI or an editor shows up.
    fn ssh_config(&self) -> Result<SshConfig> {
        SshConfig::load(&self.ssh_config)
            .with_context(|| format!("cannot read {}", self.ssh_config.display()))
    }

    /// The TUI's database, made on first use the way the TUI makes it.
    fn db(&self) -> Result<Db> {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.data_dir)?;
        Ok(Db::open(&self.data_dir.join("vssh.db"))?)
    }

    fn hosts(&self) -> Result<Vec<Host>> {
        let sshc = self.ssh_config()?;
        let db = self.db()?;
        db.import(&sshc.hosts())?;
        Ok(store::hosts(&sshc, &db.metas()?))
    }

    /// A host of the config, resolved the way `ssh alias` would. Only those:
    /// the window cannot make the app connect to a name it made up.
    fn target(&self, alias: &str) -> Result<Target> {
        let sshc = self.ssh_config()?;
        let known = sshc.hosts().iter().any(|host| host == alias);
        ensure!(known, "no host named {alias:?}");
        Target::from_config(&sshc, alias)
    }
}

#[tauri::command(async)]
fn list_hosts(backend: State<'_, Backend>) -> Reply<Vec<Host>> {
    Ok(backend.hosts()?)
}

#[tauri::command]
async fn get_host_stats(alias: String, backend: State<'_, Backend>) -> Reply<Peek> {
    let target = backend.target(&alias)?;
    backend.net.peek(alias, target).await
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

#[tauri::command(async)]
fn list_snippets(backend: State<'_, Backend>) -> Reply<Vec<Snippet>> {
    Ok(backend.db().and_then(|db| Ok(db.snippets()?))?)
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
            resolve_conn_error
        ])
}

/// Opens the window over the user's own hosts and runs until it is closed.
pub fn run() {
    let backend = Backend::new(ssh_config_path(), data_dir(), Core);
    builder::<tauri::Wry>(backend)
        .run(tauri::generate_context!())
        .expect("vssh could not start its window");
}
