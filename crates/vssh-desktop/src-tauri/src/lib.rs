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
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, ensure, Context, Result};
use serde::Serialize;
use tauri::State;
use vssh_core::config::{data_dir, ssh_config_path, Lang};
use vssh_core::conn_err::{ConnError, Report};
use vssh_core::ssh::{self, SessionCmd, Target, Via};
use vssh_core::stats::Peek;
use vssh_core::store::db::{Db, Snippet};
use vssh_core::store::ssh_config::SshConfig;
use vssh_core::store::{self, Host};
use vssh_core::task::{self, Event, Peeks, Receiver, Shell};

mod session;

/// What a command answers: its value, or a failure the window can show.
type Reply<T> = Result<T, ConnError>;

/// An answer from the network, some time later.
pub type Answer<T> = Pin<Box<dyn Future<Output = Reply<T>> + Send>>;

/// How long a TCP connect to a host took, in ms; `None` if it is unreachable.
pub type Ping = Pin<Box<dyn Future<Output = Option<u64>> + Send>>;

/// Where a running shell reports to.
pub type Heard = Arc<dyn Fn(Shell) + Send + Sync>;

/// The network, as far as the commands use it. The app's is [`Core`]; the
/// contract tests put one in its place that answers from memory.
pub trait Transport: Send + Sync + 'static {
    /// One round of metrics from a host of the ssh config.
    fn peek(&self, alias: String, target: Target) -> Answer<Peek>;
    /// Times a TCP connect to the target itself.
    fn ping(&self, target: Target) -> Ping;
    /// Whether the target's host key is on record: it was connected to before.
    fn known(&self, target: &Target) -> bool;
    /// A shell on a PTY of that size, until either side ends it; dropping
    /// the answer ends it. `keys` types into it and resizes it.
    fn shell(
        &self,
        target: Target,
        size: (u16, u16),
        keys: Receiver<SessionCmd>,
        report: Heard,
    ) -> Answer<()>;
    /// A person's yes to a host key that does not match the one on record,
    /// for the next connection to the target.
    fn trust(&self, target: &Target, fingerprint: &str, forever: bool);
}

/// The real network: `vssh_core::task`, on Tauri's own tokio runtime. It
/// keeps one connection per host it was asked about.
#[derive(Default)]
pub struct Core(Peeks);

impl Transport for Core {
    fn peek(&self, alias: String, target: Target) -> Answer<Peek> {
        let peeks = self.0.clone();
        // Keys and the agent only: the vault has no window yet (D6).
        Box::pin(async move { peeks.ask(&alias, target).await })
    }

    fn ping(&self, target: Target) -> Ping {
        Box::pin(async move {
            let (tx, mut rx) = task::channel();
            task::ping(String::new(), target, move |ev| drop(tx.send(ev)));
            match rx.recv().await {
                Some(Event::Ping(_, ms)) => ms.map(|ms| ms as u64),
                _ => None,
            }
        })
    }

    fn known(&self, target: &Target) -> bool {
        ssh::is_known(target)
    }

    fn shell(
        &self,
        target: Target,
        size: (u16, u16),
        keys: Receiver<SessionCmd>,
        report: Heard,
    ) -> Answer<()> {
        // Keys and the agent only, as `peek`.
        Box::pin(task::shell(target, size, keys, move |ev| report(ev)))
    }

    fn trust(&self, target: &Target, fingerprint: &str, forever: bool) {
        ssh::trust_host_key(&target.host, target.port, fingerprint, forever);
    }
}

/// What the commands work on: where the user's hosts are, and the network.
pub struct Backend {
    ssh_config: PathBuf,
    data_dir: PathBuf,
    net: Box<dyn Transport>,
    sessions: Mutex<session::Sessions>,
}

impl Backend {
    pub fn new(ssh_config: PathBuf, data_dir: PathBuf, net: impl Transport) -> Self {
        Self {
            ssh_config,
            data_dir,
            net: Box::new(net),
            sessions: Mutex::default(),
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

/// One round of metrics from a host. `passive` is the dashboard asking on
/// its own, not a person: it gets `None` where an answer would take trusting
/// a host key nobody has seen, or logging in to a jump host. The core's
/// `task::probe` draws the same line for the TUI.
#[tauri::command]
async fn get_host_stats(
    alias: String,
    passive: Option<bool>,
    backend: State<'_, Backend>,
) -> Reply<Option<Peek>> {
    let target = backend.target(&alias)?;
    let committing = target.via != Via::Direct || !backend.net.known(&target);
    if passive == Some(true) && committing {
        return Ok(None);
    }
    backend.net.peek(alias, target).await.map(Some)
}

/// How long a TCP connect to the host takes, in ms; `None` if it is down.
/// Refused for a host behind a jump host or a proxy command: its own address
/// may mean nothing from here, and "down" would be a guess.
#[tauri::command]
async fn ping_host(alias: String, backend: State<'_, Backend>) -> Reply<Option<u64>> {
    let target = backend.target(&alias)?;
    if target.via != Via::Direct {
        return Err(anyhow!("{alias:?} is not reached directly").into());
    }
    Ok(backend.net.ping(target).await)
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
            ping_host,
            session::session_open,
            session::quick_connect,
            session::session_start,
            session::session_write,
            session::session_resize,
            session::session_close,
            session::session_trust,
            list_snippets,
            resolve_conn_error
        ])
}

/// Opens the window over the user's own hosts and runs until it is closed.
pub fn run() {
    let backend = Backend::new(ssh_config_path(), data_dir(), Core::default());
    builder::<tauri::Wry>(backend)
        // Here and not in `builder`: it wants a display, which tests have not.
        .plugin(tauri_plugin_clipboard_manager::init())
        .run(tauri::generate_context!())
        .expect("vssh could not start its window");
}
