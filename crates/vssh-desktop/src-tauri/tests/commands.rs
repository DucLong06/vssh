//! The contract between the window and the shell: each command is called the
//! way the webview calls it, over Tauri's IPC on the mock runtime, and its
//! JSON answer is what `src/lib/api.ts` declares. Nothing here opens a
//! socket: the network is a [`Transport`] that answers from memory.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_context, noop_assets, MockRuntime};
use tauri::webview::InvokeRequest;
use tauri::{Listener, Manager, WebviewWindow, WebviewWindowBuilder};
use vssh_core::config::Lang;
use vssh_core::conn_err::{ConnError, Fail};
use vssh_core::sftp::{Cmd, Entry};
use vssh_core::ssh::{SessionCmd, Target};
use vssh_core::stats::parse_peek;
use vssh_core::stats::{Peek, Stats};
use vssh_core::store::db::{Db, Meta, Snippet};
use vssh_core::store::ssh_config::Forward;
use vssh_core::store::Host;
use vssh_core::task::{Receiver, Shell};
use vssh_desktop::{
    builder, Answer, Backend, Heard, Password, Ping, Progress, Ready, Remote, Transport,
};

#[path = "commands/files.rs"]
mod files;
#[path = "commands/forwards.rs"]
mod forwards;
#[path = "commands/hosts.rs"]
mod hosts;
#[path = "commands/key_setup.rs"]
mod key_setup;
#[path = "commands/real.rs"]
mod real;
/// Terminal sessions.
#[path = "commands/session.rs"]
mod session;
#[path = "commands/snippets.rs"]
mod snippets;
#[path = "commands/update.rs"]
mod update;
#[path = "commands/vault.rs"]
mod vault;

/// What a command run on a host answers.
type Exec = dyn Fn(&Target, &str) -> Result<String, ConnError> + Send + Sync;

const SSH_CONFIG: &str = "\
Host web-01
  HostName 10.0.0.5
  User deploy
  Port 2222

Host bare

Host *.lan
  User ops
";

/// A network that answers from memory and remembers what it was asked.
/// Every host is one it has met, 12 ms away.
#[derive(Clone)]
struct Fake {
    /// What every peek answers.
    answer: Result<Peek, ConnError>,
    asked: Arc<Mutex<Vec<String>>>,
    /// What every SFTP command answers, once it has reported `steps`.
    files: Result<Vec<Entry>, ConnError>,
    steps: Vec<(u64, u64)>,
    /// While set, a transfer reports its steps and then never ends.
    stall: Arc<AtomicBool>,
    /// A stalled transfer or a running forward was dropped: it stopped
    /// where it was.
    dropped: Arc<AtomicBool>,
    /// Why a forward ends before it listens; `None` = it listens for good.
    forward: Arc<Mutex<Option<ConnError>>>,
    /// What a command answers on a host, after `pace`.
    exec: Arc<Exec>,
    pace: Duration,
    /// How many commands run right now, and the most there ever were.
    busy: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    /// What the next logins with a key alone answer; after these, yes.
    key_logins: Arc<Mutex<VecDeque<Result<(), ConnError>>>>,
    /// Why no host can be logged in to for commands; `None` = each can.
    down: Option<ConnError>,
    known: bool,
    ping: Option<u64>,
}

/// Says so when it is dropped.
struct Flag(Arc<AtomicBool>);

impl Drop for Flag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Whom a command went to, and the password it was given to log in with.
fn at(target: &Target, password: &Password) -> String {
    let at = format!("{}@{}:{}", target.user, target.host, target.port);
    match password {
        Some(password) => format!("{at} ({})", password.as_str()),
        None => at,
    }
}

impl Fake {
    fn answering(answer: Result<Peek, ConnError>) -> Self {
        Self {
            answer,
            asked: Arc::default(),
            files: Ok(vec![]),
            steps: vec![],
            stall: Arc::default(),
            dropped: Arc::default(),
            forward: Arc::default(),
            exec: Arc::new(|_, _| Ok(String::new())),
            pace: Duration::ZERO,
            busy: Arc::default(),
            peak: Arc::default(),
            key_logins: Arc::default(),
            down: None,
            known: true,
            ping: Some(12),
        }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }

    fn silent() -> Self {
        Self::answering(Ok(Peek::default()))
    }
}

impl Transport for Fake {
    fn peek(&self, alias: String, target: Target, password: Password) -> Answer<Peek> {
        let line = format!("{alias} = {}", at(&target, &password));
        self.asked.lock().unwrap().push(line);
        let answer = self.answer.clone();
        Box::pin(async move { answer })
    }

    fn sftp(
        &self,
        target: Target,
        password: Password,
        cmd: Cmd,
        progress: Progress,
    ) -> Answer<Vec<Entry>> {
        let line = format!("{} {cmd:?}", at(&target, &password));
        self.asked.lock().unwrap().push(line);
        let fake = self.clone();
        Box::pin(async move {
            for (done, total) in fake.steps {
                progress(done, total);
            }
            if fake.stall.load(Ordering::SeqCst) {
                let _stopped = Flag(fake.dropped);
                std::future::pending::<()>().await;
            }
            fake.files
        })
    }

    fn forward(
        &self,
        target: Target,
        password: Password,
        fwd: Forward,
        ready: Ready,
    ) -> Answer<()> {
        let line = format!(
            "{} forward {} {}",
            at(&target, &password),
            fwd.kind,
            fwd.value()
        );
        self.asked.lock().unwrap().push(line);
        let fake = self.clone();
        Box::pin(async move {
            if let Some(why) = fake.forward.lock().unwrap().clone() {
                return Err(why);
            }
            ready();
            let _stopped = Flag(fake.dropped);
            std::future::pending().await
        })
    }

    fn connect(&self, target: Target, password: Password) -> Answer<Remote> {
        let fake = self.clone();
        Box::pin(async move {
            if let Some(why) = fake.down.clone() {
                return Err(why);
            }
            let exec = move |command: String| -> Answer<String> {
                let (fake, target) = (fake.clone(), target.clone());
                // Asked once the command really starts.
                let line = format!("{} exec {command}", at(&target, &password));
                Box::pin(async move {
                    fake.asked.lock().unwrap().push(line);
                    let busy = fake.busy.fetch_add(1, Ordering::SeqCst) + 1;
                    fake.peak.fetch_max(busy, Ordering::SeqCst);
                    tokio::time::sleep(fake.pace).await;
                    fake.busy.fetch_sub(1, Ordering::SeqCst);
                    (fake.exec)(&target, &command)
                })
            };
            Ok(Box::new(exec) as Remote)
        })
    }

    fn key_login(&self, target: Target, key: PathBuf) -> Answer<()> {
        let line = format!("{} key {}", at(&target, &None), key.display());
        self.asked.lock().unwrap().push(line);
        let answer = self.key_logins.lock().unwrap().pop_front();
        Box::pin(async move { answer.unwrap_or(Ok(())) })
    }

    fn ping(&self, target: Target) -> Ping {
        let line = format!("ping {}:{}", target.host, target.port);
        self.asked.lock().unwrap().push(line);
        let ms = self.ping;
        Box::pin(async move { ms })
    }

    fn known(&self, _: &Target) -> bool {
        self.known
    }

    /// A shell that says `$ `, echoes what is typed, runs `sleep` and ends
    /// on `exit`; or refuses, if that is this network's answer to everything.
    fn shell(
        &self,
        target: Target,
        (cols, rows): (u16, u16),
        mut keys: Receiver<SessionCmd>,
        report: Heard,
    ) -> Answer<()> {
        let at = format!("{}@{}:{}", target.user, target.host, target.port);
        let asked = self.asked.clone();
        let note = move |line: String| asked.lock().unwrap().push(line);
        note(format!("shell {at} {cols}x{rows}"));
        let refusal = self.answer.clone().err();
        Box::pin(async move {
            // Dropped with the shell, however it ends.
            let _gone = Gone(Box::new(note.clone()), at);
            if let Some(refusal) = refusal {
                return Err(refusal);
            }
            report(Shell::Up);
            report(Shell::Data(b"$ ".to_vec()));
            while let Some(cmd) = keys.recv().await {
                match cmd {
                    SessionCmd::Data(typed) if typed == b"exit\r" => break,
                    SessionCmd::Data(typed) => {
                        let running = typed.starts_with(b"sleep").then(|| "sleep".to_string());
                        report(Shell::Data(typed));
                        report(Shell::Busy(running));
                    }
                    SessionCmd::Resize(cols, rows) => note(format!("resize {cols}x{rows}")),
                }
            }
            Ok(())
        })
    }

    fn trust(&self, target: &Target, fingerprint: &str, forever: bool) {
        let host = format!("{}:{}", target.host, target.port);
        let line = format!("trust {host} {fingerprint} forever={forever}");
        self.asked.lock().unwrap().push(line);
    }

    fn forget(&self, alias: Option<&str>) {
        let line = format!("forget {}", alias.unwrap_or("every host"));
        self.asked.lock().unwrap().push(line);
    }
}

/// A failure the connect path tagged, as a command answers with it.
fn failed(fail: Fail) -> ConnError {
    ConnError::from(anyhow::Error::new(fail))
}

/// Says when a shell's connection is let go of.
struct Gone(Box<dyn Fn(String) + Send>, String);

impl Drop for Gone {
    fn drop(&mut self) {
        (self.0)(format!("gone {}", self.1));
    }
}

/// A host that is reached through another one.
const HIDDEN: &str = "\nHost hidden\n  HostName 10.9.9.9\n  User deploy\n  ProxyJump web-01\n";

fn with_hidden(home: &Path) {
    fs::write(home.join("config"), format!("{SSH_CONFIG}{HIDDEN}")).unwrap();
}

/// A home of its own for one test: an ssh config and an empty data directory.
fn home(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vssh-desktop-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("data")).unwrap();
    fs::write(dir.join("config"), SSH_CONFIG).unwrap();
    dir
}

fn db(home: &Path) -> Db {
    Db::open(&home.join("data/vssh.db")).unwrap()
}

/// The app over that home, and its window. The app must outlive the window.
fn window(home: &Path, net: Fake) -> (tauri::App<MockRuntime>, WebviewWindow<MockRuntime>) {
    open(Backend::new(home.join("config"), home.join("data"), net))
}

fn open(backend: Backend) -> (tauri::App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let app = builder(backend).build(mock_context(noop_assets())).unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    (app, window)
}

/// Calls a command as the webview does; the error side is what the
/// frontend's `invoke` promise rejects with.
fn call(window: &WebviewWindow<MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
    let request = InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        // The origin of the app's own pages: Windows has them under http.
        url: if cfg!(windows) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
        .parse()
        .unwrap(),
        body: InvokeBody::Json(args),
        headers: Default::default(),
        invoke_key: window.app_handle().invoke_key().to_string(),
    };
    get_ipc_response(window, request).map(|body| body.deserialize::<Value>().unwrap())
}

/// Everything the app emits under one event name, as the window would hear it.
fn listen(app: &tauri::App<MockRuntime>, event: &str) -> mpsc::Receiver<Value> {
    let (tx, rx) = mpsc::channel();
    app.listen(event, move |ev| {
        drop(tx.send(serde_json::from_str(ev.payload()).unwrap()))
    });
    rx
}

/// The next event, which must come.
fn next(events: &mpsc::Receiver<Value>) -> Value {
    events.recv_timeout(Duration::from_secs(5)).unwrap()
}

/// Waits for what the runtime does in its own time, such as dropping work
/// that was stopped.
fn until(what: &str, done: impl Fn() -> bool) {
    for _ in 0..500 {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("never happened: {what}");
}

/// The shape of an error that is not a connection failure.
fn other(words: &str) -> Value {
    json!({ "class": "other", "fail": null, "chain": [words] })
}

#[test]
fn list_hosts_reads_the_ssh_config_and_the_database() {
    let home = home("list-hosts");
    let meta = Meta {
        grp: "prod".into(),
        tags: "web eu".into(),
        pinned: true,
        ..Meta::default()
    };
    db(&home).set_meta("web-01", &meta).unwrap();
    let (_app, window) = window(&home, Fake::silent());

    let empty = json!({
        "grp": "", "tags": "", "notes": "", "pinned": false, "last_connected": 0, "connects": 0,
        "paused": false,
    });
    assert_eq!(
        call(&window, "list_hosts", json!({})),
        Ok(json!([
            {
                "alias": "web-01", "hostname": "10.0.0.5", "user": "deploy", "port": 2222,
                "meta": {
                    "grp": "prod", "tags": "web eu", "notes": "", "pinned": true,
                    "last_connected": 0, "connects": 0, "paused": false,
                },
            },
            // A host with nothing but a name, and no row in the database yet.
            { "alias": "bare", "hostname": null, "user": null, "port": null, "meta": empty },
        ]))
    );
    // Like the TUI, listing gives every host its row.
    assert_eq!(db(&home).metas().unwrap().len(), 2);
}

#[test]
fn list_hosts_without_a_config_is_empty_and_an_unreadable_one_is_an_error() {
    let home = home("list-hosts-none");
    fs::remove_file(home.join("config")).unwrap();
    let (_app, window) = window(&home, Fake::silent());
    assert_eq!(call(&window, "list_hosts", json!({})), Ok(json!([])));

    // A directory where the file should be: it exists and cannot be read.
    fs::create_dir(home.join("config")).unwrap();
    let err = call(&window, "list_hosts", json!({})).unwrap_err();
    assert_eq!(err["class"], "other");
    let chain = err["chain"].as_array().unwrap();
    assert_eq!(chain.len(), 2, "{err}");
    assert!(
        chain[0].as_str().unwrap().starts_with("cannot read "),
        "{err}"
    );
}

#[test]
fn get_host_stats_peeks_at_the_host_the_config_names() {
    let home = home("stats");
    let peek = Peek {
        stats: Stats {
            cpu: Some(42),
            mem: Some((2_000_000, 8_000_000)),
            disk: None,
        },
        load: Some("0.84 0.50 0.30".into()),
        uptime: Some("3 days, 2:15".into()),
        os: Some("Linux 6.8 x86_64".into()),
        host: None,
    };
    let net = Fake::answering(Ok(peek));
    let (_app, window) = window(&home, net.clone());
    assert_eq!(
        call(&window, "get_host_stats", json!({ "alias": "web-01" })),
        Ok(json!({
            "stats": { "cpu": 42, "mem": [2000000, 8000000], "disk": null },
            "load": "0.84 0.50 0.30",
            "uptime": "3 days, 2:15",
            "os": "Linux 6.8 x86_64",
            "host": null,
        }))
    );
    assert_eq!(
        *net.asked.lock().unwrap(),
        ["web-01 = deploy@10.0.0.5:2222"]
    );
}

#[test]
fn get_host_stats_refuses_a_host_that_is_not_in_the_config() {
    let home = home("stats-missing");
    let net = Fake::silent();
    let (_app, window) = window(&home, net.clone());
    for alias in ["nope", "", "*.lan"] {
        assert_eq!(
            call(&window, "get_host_stats", json!({ "alias": alias })),
            Err(other(&format!("no host named {alias:?}"))),
        );
    }
    // The argument itself missing or of the wrong type never reaches the command.
    for args in [json!({}), json!({ "alias": 7 })] {
        let err = call(&window, "get_host_stats", args).unwrap_err();
        assert!(err.as_str().unwrap().contains("alias"), "{err}");
    }
    // Nobody was connected to for any of that.
    assert!(net.asked().is_empty());
}

#[test]
fn get_host_stats_passes_a_connection_failure_on_with_its_class() {
    let home = home("stats-refused");
    let refused = Fail::Refused {
        addr: "10.0.0.5:2222".into(),
    };
    let err =
        ConnError::from(anyhow::anyhow!("Connection refused (os error 111)").context(refused));
    let (_app, window) = window(&home, Fake::answering(Err(err)));
    assert_eq!(
        call(&window, "get_host_stats", json!({ "alias": "web-01" })),
        Err(json!({
            "class": "refused",
            "fail": { "kind": "refused", "addr": "10.0.0.5:2222" },
            "chain": ["connection refused by 10.0.0.5:2222", "Connection refused (os error 111)"],
        }))
    );
}

/// The dashboard asks on its own, every few seconds, about every host. That
/// must trust nothing new and log in nowhere the user did not go before.
#[test]
fn a_passive_get_host_stats_leaves_hosts_alone_that_asking_would_commit_to() {
    let home = home("stats-passive");
    with_hidden(&home);
    let stats = |window: &WebviewWindow<MockRuntime>, alias: &str, passive: bool| {
        let args = json!({ "alias": alias, "passive": passive });
        call(window, "get_host_stats", args).map(|peek| !peek.is_null())
    };

    // A host whose key is not on record: connecting would record it.
    let net = Fake {
        known: false,
        ..Fake::silent()
    };
    let (_app, stranger) = window(&home, net.clone());
    assert_eq!(stats(&stranger, "web-01", true), Ok(false));
    assert!(net.asked.lock().unwrap().is_empty());
    // Asked for by a person, it is peeked at, as `v` does in the TUI.
    assert_eq!(stats(&stranger, "web-01", false), Ok(true));
    assert_eq!(net.asked.lock().unwrap().len(), 1);

    let net = Fake::silent();
    let (_app, met) = window(&home, net.clone());
    assert_eq!(stats(&met, "web-01", true), Ok(true));
    // Behind a jump host: a round would log in to the jump host too.
    assert_eq!(stats(&met, "hidden", true), Ok(false));
    assert_eq!(stats(&met, "hidden", false), Ok(true));
    assert_eq!(
        *net.asked.lock().unwrap(),
        [
            "web-01 = deploy@10.0.0.5:2222",
            "hidden = deploy@10.9.9.9:22"
        ]
    );
    // What is not a host is refused either way.
    assert!(stats(&met, "nope", true).is_err());
}

/// D14: a host that is not watched is asked for nothing on the dashboard's
/// own, and no connection is kept for it; the same for every host at once.
/// Both are in the database, so they are as they were when the app is back.
#[test]
fn a_host_that_is_not_watched_is_left_alone_until_a_person_asks() {
    let home = home("watch");
    let net = Fake::silent();
    let (_app, up) = window(&home, net.clone());
    let stats = |alias: &str, passive: bool| {
        let args = json!({ "alias": alias, "passive": passive });
        call(&up, "get_host_stats", args).map(|peek| !peek.is_null())
    };
    let watch = |alias: Value, paused: bool| {
        call(
            &up,
            "watch_set",
            json!({ "alias": alias, "paused": paused }),
        )
    };
    let paused = |alias: &str| {
        let metas = db(&home).metas().unwrap();
        metas.get(alias).is_some_and(|meta| meta.paused)
    };

    assert_eq!(call(&up, "watch_paused", json!({})), Ok(json!(false)));
    assert_eq!(stats("web-01", true), Ok(true));
    // One host: its connection is let go of, and the dashboard's round gets nothing.
    assert_eq!(watch(json!("web-01"), true), Ok(Value::Null));
    assert!(paused("web-01") && !paused("bare"));
    assert_eq!(stats("web-01", true), Ok(false));
    assert_eq!(stats("bare", true), Ok(true));
    // A person asking is answered, once: nothing is kept for a next round.
    assert_eq!(stats("web-01", false), Ok(true));
    let me = std::env::var("USER").unwrap_or_else(|_| "root".into());
    assert_eq!(
        net.asked(),
        [
            "web-01 = deploy@10.0.0.5:2222".to_string(),
            "forget web-01".to_string(),
            format!("bare = {me}@bare:22"),
            "web-01 = deploy@10.0.0.5:2222".to_string(),
            "forget web-01".to_string(),
        ]
    );
    // The list says so, which is where a card has it from.
    let hosts = call(&up, "list_hosts", json!({})).unwrap();
    assert_eq!(hosts[0]["meta"]["paused"], true);
    assert_eq!(hosts[1]["meta"]["paused"], false);
    // No host that the config does not have.
    assert_eq!(
        watch(json!("nowhere"), true),
        Err(other("no host named \"nowhere\""))
    );

    // Every host: the one switch over them all, which leaves each its own.
    net.asked.lock().unwrap().clear();
    assert_eq!(watch(Value::Null, true), Ok(Value::Null));
    assert_eq!(call(&up, "watch_paused", json!({})), Ok(json!(true)));
    assert_eq!(stats("bare", true), Ok(false));
    assert_eq!(net.asked(), ["forget every host"]);
    assert!(!paused("bare"));

    // The app comes back as it was left.
    drop(up);
    let (_app, again) = window(&home, Fake::silent());
    assert_eq!(call(&again, "watch_paused", json!({})), Ok(json!(true)));
    let resume = json!({ "alias": null, "paused": false });
    assert_eq!(call(&again, "watch_set", resume), Ok(Value::Null));
    let passive = |alias: &str| json!({ "alias": alias, "passive": true });
    let asked = |alias: &str| call(&again, "get_host_stats", passive(alias)).map(|p| !p.is_null());
    assert_eq!(asked("bare"), Ok(true));
    // web-01 is still its own: watched again only when it is said so.
    assert_eq!(asked("web-01"), Ok(false));
    let own = json!({ "alias": "web-01", "paused": false });
    assert_eq!(call(&again, "watch_set", own), Ok(Value::Null));
    assert_eq!(asked("web-01"), Ok(true));
}

#[test]
fn ping_host_times_the_way_to_a_host_that_is_reached_directly() {
    let home = home("ping");
    with_hidden(&home);
    let net = Fake::silent();
    let (_app, up) = window(&home, net.clone());
    let ping = |window: &WebviewWindow<MockRuntime>, alias: &str| {
        call(window, "ping_host", json!({ "alias": alias }))
    };
    assert_eq!(ping(&up, "web-01"), Ok(json!(12)));
    assert_eq!(*net.asked.lock().unwrap(), ["ping 10.0.0.5:2222"]);
    // Its own address may not be reachable from here at all: no answer
    // rather than "down".
    assert_eq!(
        ping(&up, "hidden"),
        Err(other("\"hidden\" is not reached directly"))
    );
    assert_eq!(ping(&up, "nope"), Err(other("no host named \"nope\"")));
    assert_eq!(net.asked.lock().unwrap().len(), 1);

    let net = Fake {
        ping: None,
        ..Fake::silent()
    };
    let (_app, down) = window(&home, net);
    assert_eq!(ping(&down, "web-01"), Ok(Value::Null));
}

#[test]
fn list_snippets_lists_global_ones_first() {
    let home = home("snippets");
    let (_app, window) = window(&home, Fake::silent());
    assert_eq!(call(&window, "list_snippets", json!({})), Ok(json!([])));

    let snippet = |host: &str, name: &str, command: &str| Snippet {
        id: 0,
        host: host.into(),
        name: name.into(),
        command: command.into(),
    };
    let db = db(&home);
    db.save_snippet(&snippet("web-01", "logs", "journalctl -fu nginx"))
        .unwrap();
    db.save_snippet(&snippet("", "up", "uptime")).unwrap();
    assert_eq!(
        call(&window, "list_snippets", json!({})),
        Ok(json!([
            { "id": 2, "host": "", "name": "up", "command": "uptime" },
            { "id": 1, "host": "web-01", "name": "logs", "command": "journalctl -fu nginx" },
        ]))
    );
}

#[test]
fn resolve_conn_error_puts_an_error_into_the_words_of_a_language() {
    let home = home("resolve");
    let (_app, window) = window(&home, Fake::silent());
    let resolve = |error: Value, lang: &str| {
        call(
            &window,
            "resolve_conn_error",
            json!({ "error": error, "lang": lang }),
        )
    };
    let refused = json!({
        "class": "refused",
        "fail": { "kind": "refused", "addr": "10.0.0.5:22" },
        "chain": ["connection refused by 10.0.0.5:22", "Connection refused (os error 111)"],
    });
    let chain = "connection refused by 10.0.0.5:22: Connection refused (os error 111)";
    assert_eq!(
        resolve(refused.clone(), "en"),
        Ok(json!({
            "class": "refused",
            "label": "REFUSED",
            "message": "Connection refused by 10.0.0.5:22 — is sshd running?",
            "chain": chain,
        }))
    );
    assert_eq!(
        resolve(refused.clone(), "vi"),
        Ok(json!({
            "class": "refused",
            "label": "BỊ TỪ CHỐI",
            "message": "10.0.0.5:22 từ chối kết nối — sshd có đang chạy không?",
            "chain": chain,
        }))
    );
    // What a command itself refused: shown as it is, never guessed at.
    assert_eq!(
        resolve(other("no host named \"nope\""), "vi"),
        Ok(json!({
            "class": "other",
            "label": "LỖI",
            "message": "no host named \"nope\"",
            "chain": "no host named \"nope\"",
        }))
    );

    // Not an error, not a class, not a language: none of them reach the command.
    let bad = [
        (json!("connection refused"), "en"),
        (json!({ "class": "refused" }), "en"),
        (
            json!({ "class": "exploded", "fail": null, "chain": [] }),
            "en",
        ),
        (refused, "fr"),
    ];
    for (error, lang) in bad {
        let err = resolve(error.clone(), lang).unwrap_err();
        assert!(err.is_string(), "{error} in {lang}: {err}");
    }
}

#[test]
fn the_real_transport_reports_a_closed_port_as_refused() {
    // A loopback port nothing listens on.
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let target = Target::parse(&format!("nobody@127.0.0.1:{port}")).unwrap();
    let refused = json!({ "kind": "refused", "addr": format!("127.0.0.1:{port}") });
    let fail = |err: ConnError| serde_json::to_value(&err.fail).unwrap();
    use tauri::async_runtime::block_on;
    let core = vssh_desktop::Core::default();

    let answer = block_on(core.peek("closed".into(), target.clone(), None));
    assert_eq!(fail(answer.unwrap_err()), refused);
    let list = Cmd::List(".".into());
    let quiet = Box::new(|_, _| {});
    let answer = block_on(core.sftp(target.clone(), None, list, quiet));
    assert_eq!(fail(answer.unwrap_err()), refused);
    let fwd = Forward::parse('D', "1080").unwrap();
    let answer = block_on(core.forward(target.clone(), None, fwd, Box::new(|| {})));
    assert_eq!(fail(answer.unwrap_err()), refused);
    let answer = block_on(core.connect(target.clone(), None));
    assert_eq!(fail(answer.err().unwrap()), refused);
    let answer = block_on(core.key_login(target, "/no/such/key".into()));
    assert_eq!(fail(answer.unwrap_err()), refused);
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib")
}

/// A file the frontend reads that is written from here: `UPDATE_STRINGS=1
/// cargo test -p vssh-desktop` writes it again after its source changed.
fn held(file: &str, value: &Value) {
    let path = fixtures().join(file);
    if std::env::var_os("UPDATE_STRINGS").is_some() {
        let text = serde_json::to_string_pretty(value).unwrap();
        fs::write(&path, text + "\n").unwrap();
    }
    let on_disk: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(&on_disk, value, "{file}: run again with UPDATE_STRINGS=1");
}

/// The frontend's tests render `src/lib/fixtures`. Those files must be what
/// a command could really answer: each goes through the Rust type and comes
/// back the same, so a field renamed on one side fails here.
#[test]
fn the_frontend_fixtures_are_answers_the_commands_can_give() {
    let read = |file: &str| fs::read_to_string(fixtures().join("fixtures").join(file)).unwrap();
    fn same<T: serde::Serialize + serde::de::DeserializeOwned>(text: &str) -> T {
        let json: Value = serde_json::from_str(text).unwrap();
        let typed: T = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(&typed).unwrap(), json);
        typed
    }
    same::<Vec<Host>>(&read("hosts.json"));
    same::<ConnError>(&read("refused.json"));
    // The card's numbers are the core parser's reading of what a host prints.
    let peek = same::<Peek>(&read("peek.json"));
    assert_eq!(peek, parse_peek(&read("peek.txt")));

    // One failure of every class, and each in the words of both languages:
    // what the cards show for a host that is down.
    let errors = same::<Vec<ConnError>>(&read("errors.json"));
    let classes: Vec<String> = errors
        .iter()
        .map(|e| {
            serde_json::to_value(e.class)
                .unwrap()
                .as_str()
                .unwrap()
                .into()
        })
        .collect();
    // Last, the one that is no failure of a connection: a forward's port.
    let all = "dns timeout refused unreachable auth host_key jump other port_in_use";
    assert_eq!(classes.join(" "), all);
    let home = home("fixtures");
    let (_app, window) = window(&home, Fake::silent());
    let shown = |lang: &str| -> Value {
        let resolve = |error| json!({ "error": error, "lang": lang });
        let words = errors
            .iter()
            .map(|error| call(&window, "resolve_conn_error", resolve(error)).unwrap());
        words.collect()
    };
    held(
        "fixtures/shown.json",
        &json!({ "en": shown("en"), "vi": shown("vi") }),
    );
}

/// The window's words are the core's table, as JSON.
#[test]
fn the_frontend_strings_are_the_core_table() {
    let words = |lang: Lang| -> Value {
        let rows = vssh_core::i18n::STRINGS.iter();
        rows.map(|(key, en, vi)| (key.to_string(), json!(lang.pick(en, vi))))
            .collect()
    };
    held(
        "strings.json",
        &json!({ "en": words(Lang::En), "vi": words(Lang::Vi) }),
    );
}
