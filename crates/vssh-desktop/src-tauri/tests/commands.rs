//! The contract between the window and the shell: each command is called the
//! way the webview calls it, over Tauri's IPC on the mock runtime, and its
//! JSON answer is what `src/lib/api.ts` declares. Nothing here opens a
//! socket: the network is a [`Transport`] that answers from memory.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_context, noop_assets, MockRuntime};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow, WebviewWindowBuilder};
use vssh_core::config::Lang;
use vssh_core::conn_err::{ConnError, Fail};
use vssh_core::ssh::Target;
use vssh_core::stats::parse_peek;
use vssh_core::stats::{Peek, Stats};
use vssh_core::store::db::{Db, Meta, Snippet};
use vssh_core::store::Host;
use vssh_desktop::{builder, Answer, Backend, Ping, Transport};

const SSH_CONFIG: &str = "\
Host web-01
  HostName 10.0.0.5
  User deploy
  Port 2222

Host bare

Host *.lan
  User ops
";

/// A network that answers every peek with the same thing and remembers
/// whom it was asked about. Every host is one it has met, 12 ms away.
#[derive(Clone)]
struct Fake {
    answer: Result<Peek, ConnError>,
    asked: Arc<Mutex<Vec<String>>>,
    known: bool,
    ping: Option<u64>,
}

impl Fake {
    fn answering(answer: Result<Peek, ConnError>) -> Self {
        Self {
            answer,
            asked: Arc::default(),
            known: true,
            ping: Some(12),
        }
    }

    fn silent() -> Self {
        Self::answering(Ok(Peek::default()))
    }
}

impl Transport for Fake {
    fn peek(&self, alias: String, target: Target) -> Answer<Peek> {
        let line = format!("{alias} = {}@{}:{}", target.user, target.host, target.port);
        self.asked.lock().unwrap().push(line);
        let answer = self.answer.clone();
        Box::pin(async move { answer })
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
    let backend = Backend::new(home.join("config"), home.join("data"), net);
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
        // The origin of the app's own pages on Linux and macOS.
        url: "tauri://localhost".parse().unwrap(),
        body: InvokeBody::Json(args),
        headers: Default::default(),
        invoke_key: window.app_handle().invoke_key().to_string(),
    };
    get_ipc_response(window, request).map(|body| body.deserialize::<Value>().unwrap())
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
    });
    assert_eq!(
        call(&window, "list_hosts", json!({})),
        Ok(json!([
            {
                "alias": "web-01", "hostname": "10.0.0.5", "user": "deploy", "port": 2222,
                "meta": {
                    "grp": "prod", "tags": "web eu", "notes": "", "pinned": true,
                    "last_connected": 0, "connects": 0,
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
    assert!(net.asked.lock().unwrap().is_empty());
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
fn connect_host_opens_a_session_to_a_host_of_the_config_only() {
    let home = home("connect");
    let (_app, window) = window(&home, Fake::silent());
    let connect = |alias: &str| call(&window, "connect_host", json!({ "alias": alias }));
    assert_eq!(connect("web-01"), Ok(json!(1)));
    assert_eq!(connect("nope"), Err(other("no host named \"nope\"")));
    // One counter for every kind of session.
    let quick = call(
        &window,
        "quick_connect",
        json!({ "host": "db.example.com" }),
    );
    assert_eq!(quick, Ok(json!(2)));
    assert_eq!(connect("bare"), Ok(json!(3)));
}

#[test]
fn quick_connect_gives_each_session_its_own_id_and_saves_nothing() {
    let home = home("quick");
    let (_app, window) = window(&home, Fake::silent());
    let connect = |host: &str| call(&window, "quick_connect", json!({ "host": host }));
    assert_eq!(connect("deploy@10.0.0.9:2200"), Ok(json!(1)));
    assert_eq!(connect("db.example.com"), Ok(json!(2)));
    assert_eq!(connect("root@[::1]:22"), Ok(json!(3)));
    // A quick connect is not a saved host: the config is as it was.
    assert_eq!(fs::read_to_string(home.join("config")).unwrap(), SSH_CONFIG);
}

#[test]
fn quick_connect_refuses_what_is_not_a_host() {
    let home = home("quick-bad");
    let (_app, window) = window(&home, Fake::silent());
    let connect = |host: &str| call(&window, "quick_connect", json!({ "host": host }));
    let shape = "expected user@host or user@host:port";
    for bad in ["", "   ", "@host", "de ploy@host", "user@"] {
        assert_eq!(connect(bad), Err(other(shape)), "{bad:?}");
    }
    let err = connect("deploy@host:99999").unwrap_err();
    assert_eq!(err["chain"][0], "port must be a number up to 65535");
    assert!(call(&window, "quick_connect", json!({}))
        .unwrap_err()
        .is_string());
    // A refused host used up no session id.
    assert_eq!(connect("deploy@host"), Ok(json!(1)));
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
    let answer =
        tauri::async_runtime::block_on(vssh_desktop::Core::default().peek("closed".into(), target));
    let err = answer.unwrap_err();
    assert_eq!(
        serde_json::to_value(&err.fail).unwrap(),
        json!({ "kind": "refused", "addr": format!("127.0.0.1:{port}") })
    );
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
    let all = "dns timeout refused unreachable auth host_key jump other";
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
