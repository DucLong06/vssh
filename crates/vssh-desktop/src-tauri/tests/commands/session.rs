//! Terminal sessions: opened, started once the window listens, typed into,
//! resized and closed, with the shell's output and state arriving as events.

use std::time::Duration;

use vssh_core::vault::Vault;

use super::*;

/// Everything the app emits under `event`, as the window would hear it.
fn heard(app: &tauri::App<MockRuntime>, event: &str) -> Arc<Mutex<Vec<Value>>> {
    let all = Arc::new(Mutex::new(vec![]));
    let into = all.clone();
    app.listen_any(event, move |ev| {
        let payload = serde_json::from_str(ev.payload()).unwrap();
        into.lock().unwrap().push(payload);
    });
    all
}

/// The shell runs on the runtime's threads: what it does shows up shortly.
fn until(what: &str, done: impl Fn() -> bool) {
    for _ in 0..300 {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("never happened: {what}");
}

fn bytes(text: &str) -> Value {
    json!(text.as_bytes())
}

#[test]
fn a_session_is_to_a_host_of_the_config_or_a_typed_one_and_connects_nowhere_yet() {
    let home = home("session-open");
    let net = Fake::silent();
    let (_app, window) = window(&home, net.clone());
    let open = |alias: &str| call(&window, "session_open", json!({ "alias": alias }));
    let quick = |host: &str| call(&window, "quick_connect", json!({ "host": host }));
    assert_eq!(open("web-01"), Ok(json!(1)));
    assert_eq!(open("nope"), Err(other("no host named \"nope\"")));
    // One counter for every kind of session; a refused one used up no id.
    assert_eq!(quick("deploy@10.0.0.9:2200"), Ok(json!(2)));
    assert_eq!(quick("root@[::1]:22"), Ok(json!(3)));
    assert_eq!(open("bare"), Ok(json!(4)));

    let shape = "expected user@host or user@host:port";
    for bad in ["", "   ", "@host", "de ploy@host", "user@"] {
        assert_eq!(quick(bad), Err(other(shape)), "{bad:?}");
    }
    let err = quick("deploy@host:99999").unwrap_err();
    assert_eq!(err["chain"][0], "port must be a number up to 65535");
    assert!(call(&window, "quick_connect", json!({}))
        .unwrap_err()
        .is_string());

    // A quick connect is not a saved host, and nobody was connected to yet:
    // that waits for the terminal to say how big it is.
    assert_eq!(fs::read_to_string(home.join("config")).unwrap(), SSH_CONFIG);
    assert!(net.asked.lock().unwrap().is_empty());
}

#[test]
fn a_started_session_is_a_shell_output_and_state_are_events_typing_and_size_are_commands() {
    let home = home("session-flow");
    let net = Fake::silent();
    let (app, window) = window(&home, net.clone());
    let asked = || net.asked.lock().unwrap().clone();
    let id = call(&window, "session_open", json!({ "alias": "web-01" })).unwrap();
    assert_eq!(id, json!(1));
    let data = heard(&app, "pty-data:1");
    let state = heard(&app, "pty-state:1");

    let size = json!({ "id": 1, "cols": 120, "rows": 40 });
    assert_eq!(call(&window, "session_start", size), Ok(Value::Null));
    until("the prompt", || data.lock().unwrap().len() == 1);
    assert_eq!(asked(), ["shell deploy@10.0.0.5:2222 120x40"]);
    assert_eq!(*data.lock().unwrap(), [bytes("$ ")]);
    assert_eq!(*state.lock().unwrap(), [json!({ "state": "up" })]);

    // Typed text goes in as it is, in the order it was typed.
    for typed in ["l", "s", "\r", "sleep 9\r"] {
        let typing = json!({ "id": 1, "data": typed });
        assert_eq!(call(&window, "session_write", typing), Ok(Value::Null));
    }
    until("the echo", || data.lock().unwrap().len() == 5);
    let echoed = ["$ ", "l", "s", "\r", "sleep 9\r"].map(bytes);
    assert_eq!(*data.lock().unwrap(), echoed);
    // What the shell is running, and that it went back to its prompt before.
    let busy = |command: Value| json!({ "state": "busy", "command": command });
    until("sleep", || state.lock().unwrap().len() == 5);
    assert_eq!(
        state.lock().unwrap()[3..],
        [busy(Value::Null), busy(json!("sleep"))]
    );

    let size = json!({ "id": 1, "cols": 80, "rows": 24 });
    assert_eq!(call(&window, "session_resize", size), Ok(Value::Null));
    until("the resize", || asked().len() == 2);
    assert_eq!(asked()[1], "resize 80x24");

    // Closing lets go of the connection, and says nothing more about it.
    assert_eq!(
        call(&window, "session_close", json!({ "id": 1 })),
        Ok(Value::Null)
    );
    until("the end", || asked().len() == 3);
    assert_eq!(asked()[2], "gone deploy@10.0.0.5:2222");
    assert_eq!(state.lock().unwrap().len(), 5);
    let gone = Err(other("no session 1"));
    assert_eq!(
        call(&window, "session_write", json!({ "id": 1, "data": "x" })),
        gone
    );
    assert_eq!(call(&window, "session_close", json!({ "id": 1 })), gone);
    assert_eq!(
        call(
            &window,
            "session_start",
            json!({ "id": 9, "cols": 80, "rows": 24 })
        ),
        Err(other("no session 9"))
    );
}

#[test]
fn a_shell_that_ends_says_so_and_can_be_started_again() {
    let home = home("session-exit");
    let net = Fake::silent();
    let (app, window) = window(&home, net.clone());
    call(
        &window,
        "quick_connect",
        json!({ "host": "deploy@10.0.0.9" }),
    )
    .unwrap();
    let state = heard(&app, "pty-state:1");
    let start = || {
        call(
            &window,
            "session_start",
            json!({ "id": 1, "cols": 80, "rows": 24 }),
        )
    };
    let last = || state.lock().unwrap().last().cloned();

    start().unwrap();
    until("up", || last() == Some(json!({ "state": "up" })));
    // Typing before a shell is there, or after it ended, is not an error: it is lost.
    call(
        &window,
        "session_write",
        json!({ "id": 1, "data": "exit\r" }),
    )
    .unwrap();
    let closed = json!({ "state": "closed", "error": null });
    until("the clean end", || last() == Some(closed.clone()));
    call(&window, "session_write", json!({ "id": 1, "data": "ls\r" })).unwrap();

    // Reconnect: the same session, a new connection.
    start().unwrap();
    until("up again", || last() == Some(json!({ "state": "up" })));
    let shells = net.asked.lock().unwrap().clone();
    assert_eq!(
        shells,
        [
            "shell deploy@10.0.0.9:22 80x24",
            "gone deploy@10.0.0.9:22",
            "shell deploy@10.0.0.9:22 80x24",
        ]
    );
    // Started over a running shell, it replaces it: never two for one tab.
    start().unwrap();
    until("the first let go", || net.asked.lock().unwrap().len() == 5);
}

#[test]
fn a_connection_that_fails_closes_the_session_with_the_failure_and_its_class() {
    let home = home("session-jump");
    with_hidden(&home);
    let refused = Fail::Refused {
        addr: "10.0.0.5:2222".into(),
    };
    let hop = Fail::Jump {
        hop: "10.0.0.5:2222 (hop 1 of 1)".into(),
        cause: Some(Box::new(refused.clone())),
    };
    let err = anyhow::anyhow!("Connection refused (os error 111)")
        .context(refused)
        .context(hop);
    let (app, window) = window(&home, Fake::answering(Err(err.into())));
    call(&window, "session_open", json!({ "alias": "hidden" })).unwrap();
    let state = heard(&app, "pty-state:1");
    call(
        &window,
        "session_start",
        json!({ "id": 1, "cols": 80, "rows": 24 }),
    )
    .unwrap();
    until("the failure", || !state.lock().unwrap().is_empty());
    let error = json!({
        "class": "jump",
        "fail": {
            "kind": "jump",
            "hop": "10.0.0.5:2222 (hop 1 of 1)",
            "cause": { "kind": "refused", "addr": "10.0.0.5:2222" },
        },
        "chain": [
            "jump host 10.0.0.5:2222 (hop 1 of 1) failed",
            "connection refused by 10.0.0.5:2222",
            "Connection refused (os error 111)",
        ],
    });
    assert_eq!(
        *state.lock().unwrap(),
        [json!({ "state": "closed", "error": error })]
    );
    // Which the window puts into words like any other failure.
    let shown = call(
        &window,
        "resolve_conn_error",
        json!({ "error": error, "lang": "en" }),
    )
    .unwrap();
    assert_eq!(
        shown["message"],
        "Jump host 10.0.0.5:2222 (hop 1 of 1) failed: Connection refused by 10.0.0.5:2222 — is sshd running?"
    );
}

#[test]
fn a_changed_host_key_is_trusted_only_for_a_host_on_the_sessions_road() {
    let home = home("session-trust");
    with_hidden(&home);
    let net = Fake::silent();
    let (_app, window) = window(&home, net.clone());
    call(&window, "session_open", json!({ "alias": "hidden" })).unwrap();
    let trust = |id: u64, host: &str, forever: bool| {
        let yes =
            json!({ "id": id, "host": host, "fingerprint": "SHA256:abc", "forever": forever });
        call(&window, "session_trust", yes)
    };
    // The host itself, and the jump host it is reached through.
    assert_eq!(trust(1, "10.9.9.9", false), Ok(Value::Null));
    assert_eq!(trust(1, "10.0.0.5", true), Ok(Value::Null));
    assert_eq!(
        *net.asked.lock().unwrap(),
        [
            "trust 10.9.9.9:22 SHA256:abc forever=false",
            "trust 10.0.0.5:2222 SHA256:abc forever=true",
        ]
    );
    // Not a host the window made up, and not for a session that is not there.
    assert_eq!(
        trust(1, "evil.example", true),
        Err(other("session 1 does not go through \"evil.example\""))
    );
    assert_eq!(trust(2, "10.9.9.9", true), Err(other("no session 2")));
    assert_eq!(net.asked.lock().unwrap().len(), 2);
}

#[test]
fn a_session_logs_in_with_the_vaults_password_or_a_typed_one_which_it_can_keep() {
    let home = home("session-password");
    let path = home.join("data/vault");
    let mut vault = Vault::create(&path, "master").unwrap();
    vault.set("web-01", &mut "hunter2".to_string()).unwrap();
    drop(vault);
    let net = Fake::silent();
    let (_app, window) = window(&home, net.clone());
    call(&window, "session_open", json!({ "alias": "web-01" })).unwrap();
    call(&window, "quick_connect", json!({ "host": "ops@10.0.0.9" })).unwrap();
    let start = |id: u64, typed: Value| {
        let mut args = json!({ "id": id, "cols": 80, "rows": 24 });
        args.as_object_mut()
            .unwrap()
            .extend(typed.as_object().unwrap().clone());
        call(&window, "session_start", args)
    };
    // Each start, the shell it asked for: the old one is let go of meanwhile.
    let shells = || {
        let asked = std::mem::take(&mut *net.asked.lock().unwrap());
        let shells = asked.into_iter().filter(|line| line.starts_with("shell "));
        shells.collect::<Vec<_>>()
    };

    // Locked: keys and the agent, or what the person typed for this start.
    start(1, json!({})).unwrap();
    start(1, json!({ "password": "typed" })).unwrap();
    assert_eq!(
        shells(),
        [
            "shell deploy@10.0.0.5:2222 80x24",
            "shell deploy@10.0.0.5:2222 (typed) 80x24",
        ]
    );
    // Keeping it takes an open vault: nothing is written, nobody is connected to.
    let keep = json!({ "password": "typed", "save": true });
    assert_eq!(start(1, keep.clone()), Err(other("the vault is locked")));
    assert_eq!(shells(), Vec::<String>::new());

    call(&window, "vault_unlock", json!({ "passphrase": "master" })).unwrap();
    start(1, json!({})).unwrap();
    // Typed wins over the vault's, and is kept only when asked to.
    start(1, json!({ "password": "typed" })).unwrap();
    start(1, json!({})).unwrap();
    start(1, keep).unwrap();
    start(1, json!({})).unwrap();
    assert_eq!(
        shells(),
        [
            "shell deploy@10.0.0.5:2222 (hunter2) 80x24",
            "shell deploy@10.0.0.5:2222 (typed) 80x24",
            "shell deploy@10.0.0.5:2222 (hunter2) 80x24",
            "shell deploy@10.0.0.5:2222 (typed) 80x24",
            "shell deploy@10.0.0.5:2222 (typed) 80x24",
        ]
    );
    let vault = Vault::open(&path, "master").unwrap();
    assert_eq!(vault.get("web-01"), Some("typed"));

    // A quick connect is no host of the config: it has nothing in the vault,
    // and nowhere to keep a password.
    start(2, json!({ "password": "once" })).unwrap();
    assert_eq!(shells(), ["shell ops@10.0.0.9:22 (once) 80x24"]);
    let keep = json!({ "password": "once", "save": true });
    assert_eq!(
        start(2, keep),
        Err(other("a quick connect has no host to keep a password for"))
    );
    assert_eq!(shells(), Vec::<String>::new());
    assert_eq!(Vault::open(&path, "master").unwrap().names().count(), 1);
}
