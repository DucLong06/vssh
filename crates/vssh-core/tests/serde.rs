//! What crosses to a desktop shell as JSON. The shapes asserted here are the
//! ones `crates/vssh-desktop/src/lib/api.ts` declares: change both together.
#![cfg(feature = "serde")]

use std::collections::HashMap;
use std::io;

use serde_json::{json, to_value, Value};
use vssh_core::config::Lang;
use vssh_core::conn_err::{Class, ConnError, Fail};
use vssh_core::stats::{parse_peek, Peek, Stats};
use vssh_core::store::db::{Meta, Snippet};
use vssh_core::store::ssh_config::SshConfig;
use vssh_core::store::{hosts, Host};
use vssh_core::task::Event;

/// Through JSON and back, unchanged; returns the JSON for a closer look.
fn round_trip<T>(value: &T) -> Value
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = to_value(value).unwrap();
    assert_eq!(&serde_json::from_value::<T>(json.clone()).unwrap(), value);
    json
}

#[test]
fn hosts_carry_the_config_and_the_metadata() {
    let sshc = SshConfig::parse(
        "Host web-01\n  HostName 10.0.0.5\n  User deploy\n  Port 2222\n\nHost bare\n\nHost *.lan\n  User ops\n",
    );
    let meta = Meta {
        grp: "prod".into(),
        tags: "web eu".into(),
        pinned: true,
        connects: 3,
        ..Meta::default()
    };
    let metas = HashMap::from([("web-01".to_string(), meta.clone())]);
    let list = hosts(&sshc, &metas);
    // Wildcard blocks are not hosts; a host the database has not met yet
    // has empty metadata, and what the config leaves out stays empty.
    assert_eq!(
        list,
        [
            Host {
                alias: "web-01".into(),
                hostname: Some("10.0.0.5".into()),
                user: Some("deploy".into()),
                port: Some(2222),
                meta,
            },
            Host {
                alias: "bare".into(),
                hostname: None,
                user: None,
                port: None,
                meta: Meta::default(),
            },
        ]
    );
    assert_eq!(
        round_trip(&list[0]),
        json!({
            "alias": "web-01", "hostname": "10.0.0.5", "user": "deploy", "port": 2222,
            "meta": {
                "grp": "prod", "tags": "web eu", "notes": "", "pinned": true,
                "last_connected": 0, "connects": 3,
            },
        })
    );
    let bare = round_trip(&list[1]);
    assert_eq!(
        (&bare["hostname"], &bare["user"], &bare["port"]),
        (&Value::Null, &Value::Null, &Value::Null)
    );
}

#[test]
fn stats_and_snippets_round_trip() {
    let peek = parse_peek(
        "load 0.50 0.40 0.30 1/200 999\ncpus 2\nMemTotal: 8000000 kB\nMemAvailable: 6000000 kB\n\
         /dev/sda1 100 41 59 41% /\n\
         uptime 10:15 up 3 days, 2:15, 2 users, load average: 0.5\nuname Linux 6.8 x86_64\nhost web-01\n",
    );
    let json = round_trip(&peek);
    assert_eq!(json["stats"]["cpu"], 25);
    assert_eq!(json["stats"]["mem"], json!([2000000, 8000000]));
    assert_eq!(json["stats"]["disk"], 41);
    assert_eq!(json["uptime"], "3 days, 2:15");
    assert_eq!(json["host"], "web-01");
    // A host that answered nothing is all nulls, not an error.
    assert_eq!(
        round_trip(&Peek::default())["stats"],
        json!({ "cpu": null, "mem": null, "disk": null })
    );
    assert_eq!(round_trip(&Stats::default())["cpu"], Value::Null);

    let snippet = Snippet {
        id: 4,
        host: String::new(),
        name: "up".into(),
        command: "uptime".into(),
    };
    assert_eq!(
        round_trip(&snippet),
        json!({ "id": 4, "host": "", "name": "up", "command": "uptime" })
    );
    assert_eq!(to_value(Lang::Vi).unwrap(), "vi");
}

#[test]
fn every_error_class_has_its_name_and_its_fields_in_json() {
    let addr = || "203.0.113.7:22".to_string();
    let refused = Fail::Refused { addr: addr() };
    let cases = [
        (
            Fail::Dns {
                host: "db.invalid".into(),
            },
            "dns",
            json!({ "kind": "dns", "host": "db.invalid" }),
        ),
        (
            Fail::Timeout {
                addr: addr(),
                secs: 10,
            },
            "timeout",
            json!({ "kind": "timeout", "addr": addr(), "secs": 10 }),
        ),
        (
            refused.clone(),
            "refused",
            json!({ "kind": "refused", "addr": addr() }),
        ),
        (
            Fail::Unreachable { addr: addr() },
            "unreachable",
            json!({ "kind": "unreachable", "addr": addr() }),
        ),
        (
            Fail::Auth {
                user: "deploy".into(),
                host: "web-01".into(),
                keys: 2,
                password: false,
            },
            "auth",
            json!({ "kind": "auth", "user": "deploy", "host": "web-01", "keys": 2, "password": false }),
        ),
        (
            Fail::HostKey {
                host: "alpha".into(),
                fingerprint: "SHA256:abc".into(),
                line: 7,
                file: "/home/me/.ssh/known_hosts".into(),
            },
            "host_key",
            json!({
                "kind": "host_key", "host": "alpha", "fingerprint": "SHA256:abc",
                "line": 7, "file": "/home/me/.ssh/known_hosts",
            }),
        ),
        (
            Fail::PortInUse {
                addr: "127.0.0.1:8080".into(),
            },
            "port_in_use",
            json!({ "kind": "port_in_use", "addr": "127.0.0.1:8080" }),
        ),
        (
            Fail::Jump {
                hop: "bastion (hop 1 of 2)".into(),
                cause: Some(Box::new(refused)),
            },
            "jump",
            json!({
                "kind": "jump", "hop": "bastion (hop 1 of 2)",
                "cause": { "kind": "refused", "addr": addr() },
            }),
        ),
    ];
    for (fail, class, fields) in cases {
        let os = io::Error::other("the system's own words");
        let err = ConnError::from(anyhow::Error::new(os).context(fail.clone()));
        assert_eq!(err.class, fail.class());
        assert_eq!(
            round_trip(&err),
            json!({
                "class": class,
                "fail": fields,
                "chain": [fail.to_string(), "the system's own words"],
            })
        );
    }

    // An error nobody tagged: its class, no fields, its words as they are.
    let untagged = ConnError::from(anyhow::anyhow!("Serial is not implemented yet"));
    assert_eq!(untagged.class, Class::Other);
    assert_eq!(
        round_trip(&untagged),
        json!({ "class": "other", "fail": null, "chain": ["Serial is not implemented yet"] })
    );
}

#[test]
fn an_error_is_put_into_words_only_when_it_is_shown() {
    let os = io::Error::new(io::ErrorKind::ConnectionRefused, "Connection refused");
    let err = ConnError::from(anyhow::Error::new(os).context(Fail::Refused {
        addr: "10.0.0.1:22".into(),
    }));
    // The same error, after a trip through the frontend, in either language.
    let back: ConnError = serde_json::from_value(to_value(&err).unwrap()).unwrap();
    let en = back.report(Lang::En);
    assert_eq!(
        en.message,
        "Connection refused by 10.0.0.1:22 — is sshd running?"
    );
    assert_eq!(
        en.chain,
        "connection refused by 10.0.0.1:22: Connection refused"
    );
    let vi = back.report(Lang::Vi);
    assert_eq!(
        vi.message,
        "10.0.0.1:22 từ chối kết nối — sshd có đang chạy không?"
    );
    assert_eq!(
        to_value(&vi).unwrap(),
        json!({ "class": "refused", "message": vi.message, "chain": en.chain })
    );
}

#[test]
fn events_serialize_with_their_errors() {
    let err = ConnError::from(anyhow::Error::new(Fail::Timeout {
        addr: "10.0.0.1:22".into(),
        secs: 10,
    }));
    let closed = to_value(Event::Closed(7, Some(err.clone()))).unwrap();
    assert_eq!(closed["Closed"][0], 7);
    assert_eq!(closed["Closed"][1]["class"], "timeout");
    let peek = to_value(Event::Peek("web-01".into(), Err(err))).unwrap();
    assert_eq!(peek["Peek"][0], "web-01");
    assert_eq!(peek["Peek"][1]["Err"]["fail"]["secs"], 10);
}
