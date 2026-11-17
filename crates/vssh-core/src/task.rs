//! The line between the core and whatever shows it. A frontend asks for
//! background work with the functions here and hears back through its
//! [`Sink`], one [`Event`] at a time. It never spawns, awaits or times
//! anything itself, so no async runtime API reaches it: the TUI wraps a std
//! channel in its sink, a desktop shell would emit to its window.

use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use zeroize::Zeroizing;

use crate::conn_err::{ConnError, Fail};
use crate::sftp;
use crate::ssh::{self, forward, Login, SessionCmd, Target, Via};
use crate::stats::{self, Peek, Stats};
use crate::store::ssh_config::Forward;

/// The channel a frontend talks to a running session or SFTP worker with.
/// Sending never blocks and needs no runtime.
pub use tokio::sync::mpsc::{
    unbounded_channel as channel, UnboundedReceiver as Receiver, UnboundedSender as Sender,
};

/// Everything background work can report. Its failures are [`ConnError`]s:
/// already classified, and data a desktop shell can send to its window.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Event {
    /// Output of a shell session.
    Data(u64, Vec<u8>),
    /// A session's connection ended: cleanly, or with the error that ended it.
    Closed(u64, Option<ConnError>),
    /// A session could not authenticate; ask for a password and retry.
    AuthFailed(u64, ConnError),
    Sftp(u64, sftp::Reply),
    /// Host alias and TCP connect time in ms, `None` if unreachable.
    Ping(String, Option<u128>),
    /// Forward key; `Ok(true)` = now listening, anything else = it ended.
    Forward(String, Result<bool, ConnError>),
    /// One host's answer in a batch: batch id, host alias, command output.
    Batch(u64, String, Result<String, ConnError>),
    /// Session title or host alias and its metrics; `None` = it stopped answering.
    Stats(String, Option<Stats>),
    /// Host alias and the answer to a quick peek.
    Peek(String, Result<Peek, ConnError>),
}

impl Event {
    fn ended(id: u64, res: Result<()>) -> Self {
        match res.map_err(ConnError::from) {
            Ok(()) => Self::Closed(id, None),
            Err(e) if matches!(e.fail, Some(Fail::Auth { .. })) => Self::AuthFailed(id, e),
            Err(e) => Self::Closed(id, Some(e)),
        }
    }
}

/// Where background work reports to. Called from the runtime's threads, so
/// it must not block: hand the event on and return. Any such closure is one.
pub trait Sink: Clone + Send + Sync + 'static {
    fn send(&self, ev: Event);
}

impl<F: Fn(Event) + Clone + Send + Sync + 'static> Sink for F {
    fn send(&self, ev: Event) {
        self(ev)
    }
}

/// A piece of background work. Dropping this lets the work run on;
/// [`Task::abort`] ends it. The default is work that is already over.
#[derive(Default)]
pub struct Task(Option<JoinHandle<()>>);

impl Task {
    fn spawn(work: impl std::future::Future<Output = ()> + Send + 'static) -> Self {
        Self(Some(tokio::spawn(work)))
    }

    /// Stops the work where it is. It reports nothing after this.
    pub fn abort(&self) {
        if let Some(handle) = &self.0 {
            handle.abort();
        }
    }
}

/// Runs a frontend with the background runtime behind it: the functions of
/// this module work for as long as `frontend` runs, and what is still going
/// when it returns is dropped. A host that already runs tokio (Tauri) calls
/// them from its own runtime instead.
pub fn run<T>(frontend: impl FnOnce() -> T) -> io::Result<T> {
    // Two workers are plenty for I/O-bound work and keep startup cheap.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let _inside = runtime.enter();
    Ok(frontend())
}

type Password = Option<Zeroizing<String>>;

/// Opens a shell session and returns the channel that types into it. Its
/// output arrives as [`Event::Data`], its end as [`Event::Closed`] or
/// [`Event::AuthFailed`]. With `stats`, the host's metrics are reported under
/// that name every few seconds, over the session's own connection.
pub fn session(
    id: u64,
    target: Target,
    password: Password,
    size: (u16, u16),
    login: Login,
    stats: Option<String>,
    sink: impl Sink,
) -> (Task, Sender<SessionCmd>) {
    let (cmd_tx, cmd_rx) = channel();
    let task = Task::spawn(async move {
        let res = async {
            let pw = password.as_ref().map(|p| p.as_str());
            let h = ssh::connect(&target, pw, None).await?;
            let data = |d: &[u8]| sink.send(Event::Data(id, d.to_vec()));
            let title = stats.clone().unwrap_or_default();
            let report = |s| sink.send(Event::Stats(title.clone(), s));
            // The metrics ride on the session's own connection and end with it.
            tokio::select! {
                res = ssh::shell(&h, size, login, cmd_rx, data) => res,
                () = stats::poll(&h, report), if stats.is_some() => Ok(()),
            }
        };
        sink.send(Event::ended(id, res.await));
    });
    (task, cmd_tx)
}

/// Opens an SFTP session and returns the channel that commands it. Its
/// answers arrive as [`Event::Sftp`], its end like a shell session's.
pub fn sftp(
    id: u64,
    target: Target,
    password: Password,
    sink: impl Sink,
) -> (Task, Sender<sftp::Cmd>) {
    let (cmd_tx, cmd_rx) = channel();
    let task = Task::spawn(async move {
        let res = async {
            let pw = password.as_ref().map(|p| p.as_str());
            let h = ssh::connect(&target, pw, None).await?;
            sftp::worker(h, cmd_rx, |r| sink.send(Event::Sftp(id, r))).await
        };
        sink.send(Event::ended(id, res.await));
    });
    (task, cmd_tx)
}

/// Times a TCP connect to the host: [`Event::Ping`].
pub fn ping(alias: String, target: Target, sink: impl Sink) {
    tokio::spawn(async move {
        let start = Instant::now();
        let attempt = TcpStream::connect((target.host, target.port));
        let res = timeout(Duration::from_secs(3), attempt).await;
        let ms = matches!(res, Ok(Ok(_))).then(|| start.elapsed().as_millis());
        sink.send(Event::Ping(alias, ms));
    });
}

/// Asks a host for its metrics without anyone having asked to connect:
/// [`Event::Stats`], after a short rest so that passing over a host costs
/// nothing. Abort it when the interest moves on.
pub fn probe(alias: String, target: Target, sink: impl Sink) -> Task {
    // Not through a jump host or a proxy command: that would log in to other
    // machines, or run a local program, for a cursor movement.
    if target.via != Via::Direct {
        return Task::default();
    }
    Task::spawn(async move {
        sleep(Duration::from_millis(700)).await;
        // Resting on a row is not a decision to connect: only hosts whose
        // key is already on record, and keys only, never a vault password.
        if !ssh::is_known(&target) {
            return;
        }
        let stats = match ssh::connect(&target, None, None).await {
            Ok(h) => stats::fetch(&h).await,
            Err(_) => None,
        };
        sink.send(Event::Stats(alias, stats));
    })
}

/// The quick peek: one connection, one round of [`stats::PEEK`], then
/// [`Event::Peek`]. A target that did not resolve is reported the same way.
pub fn peek(alias: String, target: Result<Target>, password: Password, sink: impl Sink) {
    tokio::spawn(async move {
        let res = async {
            let pw = password.as_ref().map(|p| p.as_str());
            let h = ssh::connect(&target?, pw, None).await?;
            let asking = ssh::exec(&h, stats::PEEK);
            let out = timeout(Duration::from_secs(10), asking).await??;
            anyhow::Ok(stats::parse_peek(&out))
        };
        sink.send(Event::Peek(alias, res.await.map_err(ConnError::from)));
    });
}

/// Runs a port forward until it fails or is aborted. [`Event::Forward`]
/// under `key`: `Ok(true)` once it listens, anything else when it ends.
pub fn forward(
    key: String,
    target: Target,
    password: Password,
    fwd: Forward,
    sink: impl Sink,
) -> Task {
    Task::spawn(async move {
        let pw = password.as_ref().map(|p| p.as_str());
        let ready = || sink.send(Event::Forward(key.clone(), Ok(true)));
        let res = forward::run(&target, pw, fwd, ready).await;
        let res = res.map(|()| false).map_err(ConnError::from);
        sink.send(Event::Forward(key.clone(), res));
    })
}

/// One host's share of a batch: connect, run `command`, [`Event::Batch`].
pub fn batch(
    id: u64,
    host: String,
    target: Result<Target>,
    password: Password,
    command: String,
    sink: impl Sink,
) -> Task {
    Task::spawn(async move {
        let res = async {
            let pw = password.as_ref().map(|p| p.as_str());
            let h = ssh::connect(&target?, pw, None).await?;
            ssh::exec(&h, &command).await
        };
        sink.send(Event::Batch(id, host, res.await.map_err(ConnError::from)));
    })
}
