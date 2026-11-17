//! Snippets and batches (D5): the saved commands, and one command run on
//! many hosts at once, each of which answers on `batch-progress`.

use std::collections::HashMap;
use std::fs;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use vssh_core::conn_err::Fail;
use vssh_desktop::Backend;

use super::{call, failed, home, listen, next, open, other, window, Fake};

#[test]
fn snippets_are_created_changed_and_deleted() {
    let home = home("snippets-crud");
    let (_app, window) = window(&home, Fake::silent());
    let list = || call(&window, "snippets_list", json!({}));
    assert_eq!(list(), Ok(json!([])));

    let up = json!({ "name": " up ", "command": "uptime\n", "host": "" });
    let up = call(&window, "snippets_create", up);
    // Trimmed, as the TUI's form does, and given its id.
    assert_eq!(
        up,
        Ok(json!({ "id": 1, "host": "", "name": "up", "command": "uptime" }))
    );
    let logs = json!({ "name": "logs", "command": "journalctl -fu {{unit}}", "host": "web-01" });
    let logs = call(&window, "snippets_create", logs).unwrap();
    assert_eq!(logs["id"], 2);
    assert_eq!(list(), Ok(json!([up.clone().unwrap(), logs])));
    // The older name for the same list is still answered.
    assert_eq!(call(&window, "list_snippets", json!({})), list());

    let changed = json!({ "id": 2, "name": "logs", "command": "journalctl -n 50", "host": "" });
    let changed = call(&window, "snippets_update", changed.clone()).map(|_| changed);
    assert_eq!(list(), Ok(json!([changed.clone().unwrap(), up.unwrap()])));

    assert_eq!(
        call(&window, "snippets_delete", json!({ "id": 1 })),
        Ok(Value::Null)
    );
    assert_eq!(list(), Ok(json!([changed.unwrap()])));
}

#[test]
fn a_snippet_needs_a_name_a_command_and_a_host_that_exists() {
    let home = home("snippets-bad");
    let (_app, window) = window(&home, Fake::silent());
    let create = |name: &str, command: &str, host: &str| {
        let args = json!({ "name": name, "command": command, "host": host });
        call(&window, "snippets_create", args)
    };
    let required = other("name and command are required");
    assert_eq!(create("", "uptime", ""), Err(required.clone()));
    assert_eq!(create("up", "  ", ""), Err(required.clone()));
    assert_eq!(
        create("up", "uptime", "nope"),
        Err(other("no host named \"nope\""))
    );
    // A pattern of the config is not a host either.
    assert_eq!(
        create("up", "uptime", "*.lan"),
        Err(other("no host named \"*.lan\""))
    );

    let update = |id: i64, name: &str| {
        let args = json!({ "id": id, "name": name, "command": "uptime", "host": "" });
        call(&window, "snippets_update", args)
    };
    assert_eq!(update(7, "up"), Err(other("no snippet with id 7")));
    // Updating is never a way to create.
    assert_eq!(update(0, "up"), Err(other("no snippet with id 0")));
    assert_eq!(call(&window, "snippets_list", json!({})), Ok(json!([])));
    assert!(create("up", "uptime", "").is_ok());
    assert_eq!(update(1, ""), Err(required));
    assert_eq!(
        call(&window, "snippets_list", json!({})).unwrap()[0]["name"],
        "up"
    );
}

#[test]
fn a_batch_fills_in_its_parameters() {
    let home = home("batch-params");
    let net = Fake::silent();
    let (app, window) = window(&home, net.clone());
    let events = listen(&app, "batch-progress");
    let run = |command: &str, params: Value| {
        let args = json!({ "hosts": ["web-01"], "command": command, "params": params });
        call(&window, "snippet_run", args)
    };

    // Spaces inside the braces do not matter; braces that hold no name
    // (a Go template, say) are not parameters and stay as they are.
    let command = "systemctl restart {{unit}} && docker ps --format '{{.Names}}' | grep {{ unit }}";
    let id = run(command, json!({ "unit": "nginx", "unused": "x" })).unwrap();
    assert_eq!(next(&events)["id"], id);
    // A value is put in as it is and not looked into again.
    assert!(run(
        "echo {{a}} {{b}}",
        json!({ "a": "{{b}}", "b": "$HOME; id" })
    )
    .is_ok());
    next(&events);
    // No parameters at all is fine for a command that has none.
    let plain = json!({ "hosts": ["web-01"], "command": "uptime {{" });
    assert!(call(&window, "snippet_run", plain).is_ok());
    next(&events);
    assert_eq!(
        net.asked(),
        [
            "deploy@10.0.0.5:2222 exec systemctl restart nginx && docker ps --format '{{.Names}}' | grep nginx",
            "deploy@10.0.0.5:2222 exec echo {{b}} $HOME; id",
            "deploy@10.0.0.5:2222 exec uptime {{",
        ]
    );

    // A parameter nobody gave a value: nothing runs anywhere.
    assert_eq!(
        run("restart {{unit}} on {{env}}", json!({ "unit": "nginx" })),
        Err(other("no value for {{env}}"))
    );
    assert_eq!(run("  ", json!({})), Err(other("type a command to run")));
    let nowhere = json!({ "hosts": [], "command": "uptime", "params": {} });
    assert_eq!(
        call(&window, "snippet_run", nowhere),
        Err(other("pick at least one host"))
    );
    assert_eq!(net.asked().len(), 3);
}

/// A config of `n` hosts, `h1` at 10.0.0.1 and so on.
fn fleet(home: &std::path::Path, n: usize) -> Vec<String> {
    let block = |i| format!("Host h{i}\n  HostName 10.0.0.{i}\n  User ops\n\n");
    let config: String = (1..=n).map(block).collect();
    fs::write(home.join("config"), config).unwrap();
    (1..=n).map(|i| format!("h{i}")).collect()
}

#[test]
fn every_host_of_a_batch_answers_for_itself() {
    let home = home("batch-hosts");
    let mut hosts = fleet(&home, 5);
    let mut net = Fake::silent();
    net.exec = Arc::new(|target, command| match target.host.as_str() {
        "10.0.0.2" => Err(failed(Fail::Refused {
            addr: "10.0.0.2:22".into(),
        })),
        "10.0.0.4" => Err(failed(Fail::Auth {
            user: "ops".into(),
            host: "10.0.0.4".into(),
            keys: 2,
            password: false,
        })),
        host => Ok(format!("{host}: {command}\n")),
    });
    let (app, window) = window(&home, net);
    let events = listen(&app, "batch-progress");
    // A name that is no host of the config fails like a host that is down.
    hosts.push("ghost".into());

    let args = json!({ "hosts": hosts, "command": "uname -r", "params": {} });
    let id = call(&window, "snippet_run", args.clone()).unwrap();
    let answers: HashMap<String, Value> = (0..6)
        .map(|_| next(&events))
        .map(|ev| (ev["host"].as_str().unwrap().to_string(), ev))
        .collect();
    assert!(events.try_recv().is_err(), "one answer per host");

    for (host, ip) in [("h1", "10.0.0.1"), ("h3", "10.0.0.3"), ("h5", "10.0.0.5")] {
        let output = format!("{ip}: uname -r\n");
        let ok = json!({ "id": id, "host": host, "output": output, "error": null });
        assert_eq!(answers[host], ok);
    }
    assert_eq!(
        answers["h2"],
        json!({
            "id": id, "host": "h2", "output": null,
            "error": {
                "class": "refused",
                "fail": { "kind": "refused", "addr": "10.0.0.2:22" },
                "chain": ["connection refused by 10.0.0.2:22"],
            },
        })
    );
    assert_eq!(answers["h4"]["error"]["class"], "auth");
    assert_eq!(answers["h4"]["error"]["fail"]["keys"], 2);
    assert_eq!(answers["h4"]["output"], Value::Null);
    assert_eq!(answers["ghost"]["error"], other("no host named \"ghost\""));

    // The next batch is another one.
    let again = call(&window, "snippet_run", args).unwrap();
    assert_ne!(again, id);
    assert_eq!(next(&events)["id"], again);
}

#[test]
fn a_batch_runs_on_so_many_hosts_at_once_and_no_more() {
    let peak_of = |name: &str, cap: Option<usize>| {
        let home = home(name);
        let hosts = fleet(&home, 12);
        let mut net = Fake::silent();
        net.pace = Duration::from_millis(40);
        let backend = Backend::new(home.join("config"), home.join("data"), net.clone());
        let backend = match cap {
            Some(cap) => backend.batch_max_parallel(cap),
            None => backend,
        };
        let (app, window) = open(backend);
        let events = listen(&app, "batch-progress");
        let args = json!({ "hosts": hosts, "command": "uptime", "params": {} });
        call(&window, "snippet_run", args).unwrap();
        for _ in 0..12 {
            next(&events);
        }
        net.peak.load(Ordering::SeqCst)
    };
    assert_eq!(peak_of("batch-cap-default", None), 8);
    assert_eq!(peak_of("batch-cap-2", Some(2)), 2);
    // Zero would be a batch that never starts: it means one at a time.
    assert_eq!(peak_of("batch-cap-0", Some(0)), 1);
}
