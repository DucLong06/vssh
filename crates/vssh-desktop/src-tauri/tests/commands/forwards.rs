//! Port forwards (D5): the ones a host has in the ssh config, started and
//! stopped from the window, which hears on `forward-state` what became of
//! each.

use std::fs;
use std::sync::atomic::Ordering;

use serde_json::{json, Value};
use vssh_core::conn_err::Fail;

use super::{call, failed, home, listen, next, other, until, window, Fake};

fn row(kind: &str, port: u16, to: (&str, u16), value: &str, state: &str) -> Value {
    json!({
        "kind": kind, "bind": "127.0.0.1", "port": port, "host": to.0, "hport": to.1,
        "value": value, "state": state,
    })
}

fn state(host: &str, kind: &str, value: &str, state: &str, error: Value) -> Value {
    json!({ "host": host, "kind": kind, "value": value, "state": state, "error": error })
}

fn in_use(addr: &str) -> Value {
    json!({
        "class": "port_in_use",
        "fail": { "kind": "port_in_use", "addr": addr },
        "chain": [format!("{addr} is already in use")],
    })
}

#[test]
fn a_forward_is_added_started_stopped_and_removed() {
    let home = home("forwards");
    let net = Fake::silent();
    let (app, window) = window(&home, net.clone());
    let events = listen(&app, "forward-state");
    let list = || call(&window, "forwards_list", json!({ "host": "web-01" }));
    let add = |kind: &str, port: u16, remote: Value| {
        let args = json!({ "host": "web-01", "kind": kind, "localPort": port, "remote": remote });
        call(&window, "forward_add", args)
    };
    let on = |cmd: &str, kind: &str, value: &str| {
        let args = json!({ "host": "web-01", "kind": kind, "value": value });
        call(&window, cmd, args)
    };
    let config = || fs::read_to_string(home.join("config")).unwrap();
    assert_eq!(list(), Ok(json!([])));

    let web = row("L", 8080, ("localhost", 80), "8080 localhost:80", "off");
    assert_eq!(add("L", 8080, json!("localhost:80")), Ok(web.clone()));
    let socks = row("D", 1080, ("", 0), "1080", "off");
    assert_eq!(add("d", 1080, Value::Null), Ok(socks.clone()));
    let back = row("R", 9000, ("localhost", 3000), "9000 localhost:3000", "off");
    assert_eq!(add("R", 9000, json!("localhost:3000")), Ok(back.clone()));
    // They are lines of the host's own block, where `ssh` reads them too.
    assert!(config().contains(
        "Host web-01\n  HostName 10.0.0.5\n  User deploy\n  Port 2222\n    \
         LocalForward 8080 localhost:80\n    DynamicForward 1080\n    \
         RemoteForward 9000 localhost:3000\n"
    ));
    assert_eq!(list(), Ok(json!([web, back, socks])));

    // On: asked of the host the config names, and said to be listening.
    assert_eq!(
        on("forward_toggle", "L", "8080 localhost:80"),
        Ok(json!(true))
    );
    let up = state("web-01", "L", "8080 localhost:80", "on", Value::Null);
    assert_eq!(next(&events), up);
    assert_eq!(list().unwrap()[0]["state"], "on");
    assert_eq!(
        net.asked(),
        ["deploy@10.0.0.5:2222 forward L 8080 localhost:80"]
    );

    // Off: the forward itself is stopped, and nothing more is heard of it.
    assert!(!net.dropped.load(Ordering::SeqCst));
    assert_eq!(
        on("forward_toggle", "L", "8080 localhost:80"),
        Ok(json!(false))
    );
    until("the forward is stopped", || {
        net.dropped.swap(false, Ordering::SeqCst)
    });
    assert_eq!(list().unwrap()[0]["state"], "off");

    // Killed while it runs: stopped, and gone from the config.
    assert_eq!(
        on("forward_toggle", "L", "8080 localhost:80"),
        Ok(json!(true))
    );
    assert_eq!(next(&events), up);
    assert_eq!(
        on("forward_kill", "L", "8080 localhost:80"),
        Ok(Value::Null)
    );
    until("the forward is stopped", || {
        net.dropped.load(Ordering::SeqCst)
    });
    assert_eq!(list(), Ok(json!([back, socks])));
    assert!(!config().contains("LocalForward"));
    assert!(events.try_recv().is_err());
}

#[test]
fn a_port_that_is_taken_is_a_typed_error() {
    let home = home("forwards-taken");
    let net = Fake::silent();
    let (app, window) = window(&home, net.clone());
    let events = listen(&app, "forward-state");
    let add = |host: &str, kind: &str, port: u16, remote: Value| {
        let args = json!({ "host": host, "kind": kind, "localPort": port, "remote": remote });
        call(&window, "forward_add", args)
    };
    let toggle = |host: &str, kind: &str, value: &str| {
        let args = json!({ "host": host, "kind": kind, "value": value });
        call(&window, "forward_toggle", args)
    };

    // One host cannot listen twice on a port of this machine...
    assert!(add("web-01", "L", 8080, json!("localhost:80")).is_ok());
    let before = fs::read_to_string(home.join("config")).unwrap();
    assert_eq!(
        add("web-01", "D", 8080, Value::Null),
        Err(in_use("127.0.0.1:8080"))
    );
    assert_eq!(fs::read_to_string(home.join("config")).unwrap(), before);
    // ...but a remote forward listens over there, and another host may
    // name the same port: the two just cannot run at once.
    assert!(add("web-01", "R", 8080, json!("localhost:80")).is_ok());
    assert!(add("bare", "L", 8080, json!("db:5432")).is_ok());

    assert_eq!(toggle("web-01", "L", "8080 localhost:80"), Ok(json!(true)));
    assert_eq!(next(&events)["state"], "on");
    assert_eq!(
        toggle("bare", "L", "8080 db:5432"),
        Err(in_use("127.0.0.1:8080"))
    );
    assert_eq!(net.asked().len(), 1, "bare was never connected to");

    // Taken by another program: the system says so once the forward is
    // tried, and the window hears it with the same class.
    assert_eq!(toggle("web-01", "L", "8080 localhost:80"), Ok(json!(false)));
    let taken = failed(Fail::PortInUse {
        addr: "127.0.0.1:8080".into(),
    });
    *net.forward.lock().unwrap() = Some(taken);
    assert_eq!(toggle("bare", "L", "8080 db:5432"), Ok(json!(true)));
    let off = next(&events);
    assert_eq!(
        off,
        json!({
            "host": "bare", "kind": "L", "value": "8080 db:5432", "state": "off",
            "error": in_use("127.0.0.1:8080"),
        })
    );
    let listed = call(&window, "forwards_list", json!({ "host": "bare" })).unwrap();
    assert_eq!(listed[0]["state"], "off");

    // And it has words of its own.
    let args = json!({ "error": off["error"], "lang": "vi" });
    let shown = call(&window, "resolve_conn_error", args).unwrap();
    assert_eq!(shown["label"], "CỔNG ĐANG BẬN");
    assert_eq!(shown["class"], "port_in_use");
}

#[test]
fn forwards_refuse_what_is_not_a_forward_of_a_known_host() {
    let home = home("forwards-bad");
    let net = Fake::silent();
    let (_app, window) = window(&home, net.clone());
    let add = |kind: &str, port: u16, remote: Value| {
        let args = json!({ "host": "web-01", "kind": kind, "localPort": port, "remote": remote });
        call(&window, "forward_add", args).unwrap_err()
    };
    let shape =
        other("a forward is L or R with a port and where it leads (host:port), or D with a port");
    assert_eq!(add("X", 8080, json!("localhost:80")), shape);
    assert_eq!(add("L", 8080, Value::Null), shape);
    assert_eq!(add("L", 8080, json!("localhost")), shape);
    assert_eq!(add("L", 0, json!("localhost:80")), shape);
    // A line break cannot carry a second option into the config.
    let args = json!({
        "host": "web-01", "kind": "L", "localPort": 8080,
        "remote": "localhost:80\n  ProxyCommand reboot",
    });
    assert!(call(&window, "forward_add", args).is_ok());
    let config = fs::read_to_string(home.join("config")).unwrap();
    assert!(!config.contains("ProxyCommand"), "{config}");

    let nobody = other("no host named \"nope\"");
    for cmd in ["forwards_list", "forward_toggle", "forward_kill"] {
        let args = json!({ "host": "nope", "kind": "L", "value": "8080 localhost:80" });
        assert_eq!(call(&window, cmd, args), Err(nobody.clone()), "{cmd}");
    }
    // Only a forward the config has can be started.
    let args = json!({ "host": "web-01", "kind": "L", "value": "9999 localhost:1" });
    assert_eq!(
        call(&window, "forward_toggle", args),
        Err(other("web-01 has no forward L 9999 localhost:1"))
    );
    assert!(net.asked().is_empty());
}
