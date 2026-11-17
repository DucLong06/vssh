//! SFTP (D4): listings, the small changes, and transfers that report their
//! progress on `sftp-progress:<id>` and can be cancelled.

use std::fs;
use std::sync::atomic::Ordering;
use std::thread;

use serde_json::{json, Value};
use vssh_core::conn_err::Fail;
use vssh_core::sftp::Entry;

use super::{call, failed, home, listen, next, other, window, Fake};

fn fixture() -> Value {
    let text = include_str!("../fixtures/files.json");
    serde_json::from_str(text).unwrap()
}

#[test]
fn sftp_list_answers_with_the_entries_of_a_directory() {
    let home = home("sftp-list");
    let mut net = Fake::silent();
    // The fixture is what a listing looks like to the window: it goes
    // through the core's type on its way out.
    net.files = Ok(serde_json::from_value::<Vec<Entry>>(fixture()).unwrap());
    let (_app, window) = window(&home, net.clone());

    let list = |path: &str| {
        call(
            &window,
            "sftp_list",
            json!({ "host": "web-01", "path": path }),
        )
    };
    assert_eq!(list("/var/log"), Ok(fixture()));
    // No path is the directory the server starts in.
    assert_eq!(list(""), Ok(fixture()));
    assert_eq!(
        net.asked(),
        [
            "deploy@10.0.0.5:2222 List(\"/var/log\")",
            "deploy@10.0.0.5:2222 List(\".\")",
        ]
    );
}

#[test]
fn sftp_realpath_says_where_a_path_leads() {
    let home = home("sftp-realpath");
    let mut net = Fake::silent();
    net.files = Ok(vec![Entry {
        name: "/home/deploy".into(),
        is_dir: true,
        size: 0,
        mtime: 0,
    }]);
    let (_app, window) = window(&home, net.clone());
    let realpath = |path: &str| {
        call(
            &window,
            "sftp_realpath",
            json!({ "host": "web-01", "path": path }),
        )
    };
    // No path is the directory the server starts in: how the window learns
    // where a listing of it is.
    assert_eq!(realpath(""), Ok(json!("/home/deploy")));
    assert_eq!(realpath("/srv/.."), Ok(json!("/home/deploy")));
    assert_eq!(
        net.asked(),
        [
            "deploy@10.0.0.5:2222 Realpath(\".\")",
            "deploy@10.0.0.5:2222 Realpath(\"/srv/..\")",
        ]
    );
}

#[test]
fn the_local_commands_read_and_change_this_machine() {
    let home = home("local");
    let dir = home.join("files");
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("b.txt"), "hello").unwrap();
    let net = Fake::silent();
    let (_app, window) = window(&home, net.clone());
    let path = |name: &str| dir.join(name).to_str().unwrap().to_string();
    let names = |listing: &Value| -> Vec<String> {
        let entries = listing["entries"].as_array().unwrap().iter();
        entries
            .map(|e| e["name"].as_str().unwrap().into())
            .collect()
    };

    // The path comes back in full, whatever way it was asked for.
    let listed = call(&window, "local_list", json!({ "path": path("sub/..") })).unwrap();
    assert_eq!(listed["path"], json!(dir.canonicalize().unwrap()));
    assert_eq!(names(&listed), ["sub", "b.txt"]);
    assert_eq!(listed["entries"][0]["is_dir"], true);
    assert_eq!(listed["entries"][1]["size"], 5);
    // No path is the user's home.
    let at_home = call(&window, "local_list", json!({ "path": "" })).unwrap();
    let user_home = vssh_core::config::home().canonicalize().unwrap();
    assert_eq!(at_home["path"], json!(user_home));

    let done = |cmd: &str, args: Value| assert_eq!(call(&window, cmd, args), Ok(Value::Null));
    done("local_mkdir", json!({ "path": path("new") }));
    done(
        "local_rename",
        json!({ "from": path("b.txt"), "to": path("c.txt") }),
    );
    let listed = call(&window, "local_list", json!({ "path": path("") })).unwrap();
    assert_eq!(names(&listed), ["new", "sub", "c.txt"]);
    done("local_rm", json!({ "path": path("c.txt"), "isDir": false }));
    done("local_rm", json!({ "path": path("new"), "isDir": true }));
    let listed = call(&window, "local_list", json!({ "path": path("") })).unwrap();
    assert_eq!(names(&listed), ["sub"]);

    // What cannot be done says why, and names the path.
    let missing = call(&window, "local_list", json!({ "path": path("nope") })).unwrap_err();
    assert_eq!(missing["class"], "other");
    assert!(
        missing["chain"][0].as_str().unwrap().contains("nope"),
        "{missing}"
    );
    // Nothing of this goes near the network.
    assert!(net.asked().is_empty());
}

#[test]
fn sftp_mkdir_rm_and_rename_ask_for_exactly_that() {
    let home = home("sftp-change");
    let net = Fake::silent();
    let (_app, window) = window(&home, net.clone());
    let done = |cmd: &str, args: Value| assert_eq!(call(&window, cmd, args), Ok(Value::Null));

    done(
        "sftp_mkdir",
        json!({ "host": "web-01", "path": "/srv/new" }),
    );
    done(
        "sftp_rm",
        json!({ "host": "web-01", "path": "/srv/old", "isDir": true }),
    );
    done(
        "sftp_rm",
        json!({ "host": "web-01", "path": "/srv/a.txt", "isDir": false }),
    );
    done(
        "sftp_rename",
        json!({ "host": "web-01", "from": "/srv/a", "to": "/srv/b" }),
    );
    assert_eq!(
        net.asked(),
        [
            "deploy@10.0.0.5:2222 Mkdir(\"/srv/new\")",
            "deploy@10.0.0.5:2222 Delete(\"/srv/old\", true)",
            "deploy@10.0.0.5:2222 Delete(\"/srv/a.txt\", false)",
            "deploy@10.0.0.5:2222 Rename(\"/srv/a\", \"/srv/b\")",
        ]
    );
}

#[test]
fn sftp_refuses_a_host_that_is_not_in_the_config() {
    let home = home("sftp-missing");
    let net = Fake::silent();
    let (_app, window) = window(&home, net.clone());
    let nobody = other("no host named \"nope\"");
    let cases = [
        ("sftp_list", json!({ "host": "nope", "path": "/" })),
        ("sftp_realpath", json!({ "host": "nope", "path": "" })),
        ("sftp_mkdir", json!({ "host": "nope", "path": "/x" })),
        (
            "sftp_rm",
            json!({ "host": "nope", "path": "/x", "isDir": false }),
        ),
        (
            "sftp_rename",
            json!({ "host": "nope", "from": "/x", "to": "/y" }),
        ),
        (
            "sftp_upload",
            json!({ "host": "nope", "local": "/tmp/a", "remote": "/a", "id": "t1" }),
        ),
        (
            "sftp_download",
            json!({ "host": "nope", "remote": "/a", "local": "/tmp/a", "id": "t1" }),
        ),
    ];
    for (cmd, args) in cases {
        assert_eq!(call(&window, cmd, args), Err(nobody.clone()), "{cmd}");
    }
    assert!(net.asked().is_empty());
}

#[test]
fn a_transfer_reports_its_progress_on_its_own_event() {
    let home = home("sftp-progress");
    let mut net = Fake::silent();
    // Two chunks inside the same percent, then half, then all of it.
    net.steps = vec![(1, 1000), (2, 1000), (500, 1000), (1000, 1000)];
    let (app, window) = window(&home, net.clone());
    let up = listen(&app, "sftp-progress:up-1");
    let down = listen(&app, "sftp-progress:down_2");

    let upload =
        json!({ "host": "web-01", "local": "/tmp/a.bin", "remote": "/srv/a.bin", "id": "up-1" });
    assert_eq!(call(&window, "sftp_upload", upload), Ok(Value::Null));
    // One event per percent at most: the second chunk said nothing new.
    let steps: Vec<Value> = up.try_iter().collect();
    assert_eq!(
        steps,
        [
            json!({ "done": 1, "total": 1000 }),
            json!({ "done": 500, "total": 1000 }),
            json!({ "done": 1000, "total": 1000 }),
        ]
    );
    // Nothing of it reached the other transfer's listener.
    assert!(down.try_recv().is_err());

    let download =
        json!({ "host": "web-01", "remote": "/srv/a.bin", "local": "/tmp/b.bin", "id": "down_2" });
    assert_eq!(call(&window, "sftp_download", download), Ok(Value::Null));
    assert_eq!(down.try_iter().count(), 3);
    assert_eq!(
        net.asked(),
        [
            "deploy@10.0.0.5:2222 Upload(\"/tmp/a.bin\", \"/srv/a.bin\")",
            "deploy@10.0.0.5:2222 Download(\"/srv/a.bin\", \"/tmp/b.bin\")",
        ]
    );
}

#[test]
fn a_transfer_is_cancelled_where_it_is() {
    let home = home("sftp-cancel");
    let mut net = Fake::silent();
    net.steps = vec![(1, 4)];
    net.stall.store(true, Ordering::SeqCst);
    let (app, window) = window(&home, net.clone());
    let events = listen(&app, "sftp-progress:t1");
    let upload = || json!({ "host": "web-01", "local": "/tmp/a", "remote": "/a", "id": "t1" });
    let cancel = || call(&window, "sftp_cancel", json!({ "id": "t1" }));

    // Nothing runs yet, so there is nothing to cancel.
    assert_eq!(cancel(), Ok(json!(false)));
    let running = {
        let (window, upload) = (window.clone(), upload());
        thread::spawn(move || call(&window, "sftp_upload", upload))
    };
    // It is under way: a quarter is across, and its id is taken.
    assert_eq!(next(&events), json!({ "done": 1, "total": 4 }));
    assert_eq!(
        call(&window, "sftp_upload", upload()),
        Err(other("transfer t1 is already running"))
    );
    assert!(!net.dropped.load(Ordering::SeqCst));

    assert_eq!(cancel(), Ok(json!(true)));
    assert_eq!(running.join().unwrap(), Err(other("transfer cancelled")));
    // The transfer itself was stopped, not just no longer waited for.
    assert!(net.dropped.load(Ordering::SeqCst));
    assert_eq!(cancel(), Ok(json!(false)));

    // The id is free again, and a transfer left alone runs to its end.
    net.stall.store(false, Ordering::SeqCst);
    assert_eq!(call(&window, "sftp_upload", upload()), Ok(Value::Null));
}

#[test]
fn a_transfer_id_must_be_fit_for_an_event_name() {
    let home = home("sftp-id");
    let net = Fake::silent();
    let (_app, window) = window(&home, net.clone());
    for id in ["", "a b", "a/b", "a:b", "tệp"] {
        let upload = json!({ "host": "web-01", "local": "/tmp/a", "remote": "/a", "id": id });
        assert_eq!(
            call(&window, "sftp_upload", upload),
            Err(other("a transfer id is made of letters, digits, - and _")),
            "{id:?}"
        );
    }
    assert!(net.asked().is_empty());
}

#[test]
fn a_host_that_died_is_reported_with_its_class() {
    let home = home("sftp-dead");
    let mut net = Fake::silent();
    net.files = Err(failed(Fail::Timeout {
        addr: "10.0.0.5:2222".into(),
        secs: 10,
    }));
    // It answered for a while, then stopped.
    net.steps = vec![(1, 4)];
    let (app, window) = window(&home, net);
    let events = listen(&app, "sftp-progress:t1");
    let timeout = json!({
        "class": "timeout",
        "fail": { "kind": "timeout", "addr": "10.0.0.5:2222", "secs": 10 },
        "chain": ["10.0.0.5:2222 did not answer within 10s"],
    });

    let list = json!({ "host": "web-01", "path": "/" });
    assert_eq!(call(&window, "sftp_list", list), Err(timeout.clone()));
    let upload = || json!({ "host": "web-01", "local": "/tmp/a", "remote": "/a", "id": "t1" });
    assert_eq!(call(&window, "sftp_upload", upload()), Err(timeout.clone()));
    assert_eq!(next(&events), json!({ "done": 1, "total": 4 }));
    // A transfer that failed is over: its id can be used again.
    assert_eq!(call(&window, "sftp_upload", upload()), Err(timeout));
}
