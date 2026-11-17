//! SSH key setup (D6): a new key, onto the host, proven to open it, and
//! only then password login is turned off. `sshd_config` is backed up
//! first and put back if any later step fails.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use serde_json::{json, Value};
use vssh_core::conn_err::Fail;
use vssh_core::ssh::Target;
use vssh_core::store::ssh_config::SshConfig;

use super::{call, failed, home, listen, other, window, Fake};

const BACKUP: &str = "cp -p /etc/ssh/sshd_config /etc/ssh/sshd_config.vssh.bak";
const INSTALL: &str = "authorized_keys";
const DISABLE: &str = "PasswordAuthentication no";
const RESTORE: &str = "cp -p /etc/ssh/sshd_config.vssh.bak /etc/ssh/sshd_config";
const HOST: &str = "deploy@10.0.0.5:2222";

/// A host on which every step works, except the one whose command holds
/// `broken`: that one prints what `sudo` would and fails.
fn host(broken: &'static str) -> Fake {
    let mut net = Fake::silent();
    net.exec = Arc::new(move |_, command| {
        assert!(command.starts_with("sh -c '"), "{command}");
        Ok(match command.contains(broken) && !broken.is_empty() {
            true => "sudo: a password is required\n[exit status 1]\n".into(),
            false => "vssh-ok\n".into(),
        })
    });
    net
}

fn refused_key() -> Fail {
    Fail::Auth {
        user: "deploy".into(),
        host: "10.0.0.5".into(),
        keys: 1,
        password: false,
    }
}

/// Runs the setup; returns its answer, the steps the window heard of, and
/// which of the four remote commands were run, in order.
fn setup(name: &str, net: Fake) -> (Result<Value, Value>, Vec<String>, Vec<&'static str>) {
    let home = home(name);
    let (app, window) = window(&home, net.clone());
    let events = listen(&app, "key-setup-step");
    let answer = call(&window, "ssh_key_setup", json!({ "host": "web-01" }));
    let steps = events.try_iter().map(|ev| {
        assert_eq!(ev["host"], "web-01");
        format!(
            "{} {}",
            ev["step"].as_str().unwrap(),
            ev["state"].as_str().unwrap()
        )
    });
    let ran = net.asked().into_iter().map(|line| {
        let known = [BACKUP, RESTORE, INSTALL, DISABLE, " key "];
        let which = known.into_iter().find(|part| line.contains(part));
        which.unwrap_or_else(|| panic!("what is {line}?"))
    });
    (answer, steps.collect(), ran.collect())
}

#[test]
fn a_key_is_made_installed_proven_and_then_passwords_are_turned_off() {
    let home = home("key-setup");
    let net = host("");
    let (app, window) = window(&home, net.clone());
    let events = listen(&app, "key-setup-step");
    let key = home.join("vssh_web-01_ed25519");

    let answer = call(&window, "ssh_key_setup", json!({ "host": "web-01" }));
    assert_eq!(answer, Ok(json!(key)));
    let steps: Vec<String> = events
        .try_iter()
        .map(|ev| {
            format!(
                "{} {}",
                ev["step"].as_str().unwrap(),
                ev["state"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        steps,
        [
            "generate running",
            "generate done",
            "backup running",
            "backup done",
            "install running",
            "install done",
            "verify running",
            "verify done",
            "disable running",
            "disable done",
        ]
    );

    // The key is the user's alone, and its public half is what was sent.
    assert_eq!(
        fs::metadata(&key).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let public = fs::read_to_string(home.join("vssh_web-01_ed25519.pub")).unwrap();
    assert!(public.starts_with("ssh-ed25519 AAAA"), "{public}");
    let asked = net.asked();
    let login = format!("{HOST} key {}", key.display());
    assert_eq!(asked.len(), 5, "{asked:#?}");
    // The backup comes before anything is changed on the host.
    assert!(asked[0].contains(BACKUP), "{}", asked[0]);
    assert!(
        asked[1].contains(INSTALL) && asked[1].contains(public.trim()),
        "{}",
        asked[1]
    );
    // The key must open the host before passwords stop doing so, and
    // still after.
    assert_eq!(asked[2], login);
    assert!(
        asked[3].contains(DISABLE) && asked[3].contains("sshd -t"),
        "{}",
        asked[3]
    );
    assert_eq!(asked[4], login);
    assert!(!asked.iter().any(|line| line.contains(RESTORE)));

    // From now on the host is logged in to with that key.
    let config = SshConfig::load(&home.join("config")).unwrap();
    assert_eq!(config.own("web-01", "IdentityFile"), key.to_str().unwrap());
    let keys = Target::from_config(&config, "web-01").unwrap().keys;
    assert_eq!(keys, std::slice::from_ref(&key));

    // Asked again, the key that is there is used, not replaced.
    assert_eq!(
        call(&window, "ssh_key_setup", json!({ "host": "web-01" })),
        Ok(json!(key))
    );
    assert_eq!(
        fs::read_to_string(home.join("vssh_web-01_ed25519.pub")).unwrap(),
        public
    );
    assert!(net.asked()[6].contains(public.trim()));
    let config = fs::read_to_string(home.join("config")).unwrap();
    assert_eq!(config.matches("IdentityFile").count(), 1, "{config}");
}

#[test]
fn without_a_backup_nothing_on_the_host_is_touched() {
    let (answer, steps, ran) = setup("key-no-backup", host(BACKUP));
    let err = answer.unwrap_err();
    assert_eq!(err["class"], "other");
    assert_eq!(
        err["chain"],
        json!([
            "cannot back up sshd_config (it takes root, or sudo without a password)",
            "sudo: a password is required\n[exit status 1]"
        ])
    );
    assert_eq!(
        steps,
        [
            "generate running",
            "generate done",
            "backup running",
            "backup failed"
        ]
    );
    assert_eq!(ran, [BACKUP]);
}

#[test]
fn a_key_that_cannot_be_installed_ends_with_the_backup_restored() {
    let (answer, steps, ran) = setup("key-no-install", host(INSTALL));
    assert_eq!(
        answer.unwrap_err()["chain"][0],
        "cannot add the key to authorized_keys"
    );
    assert_eq!(
        steps[4..],
        [
            "install running",
            "install failed",
            "restore running",
            "restore done"
        ]
    );
    assert_eq!(ran, [BACKUP, INSTALL, RESTORE]);
}

#[test]
fn a_key_that_does_not_open_the_host_ends_with_the_backup_restored() {
    let net = host("");
    net.key_logins
        .lock()
        .unwrap()
        .push_back(Err(failed(refused_key())));
    let home = home("key-no-verify");
    let (app, window) = window(&home, net.clone());
    let events = listen(&app, "key-setup-step");
    let err = call(&window, "ssh_key_setup", json!({ "host": "web-01" })).unwrap_err();
    // The failure is the login's own, with its class.
    assert_eq!(err["class"], "auth");
    assert_eq!(err["fail"]["keys"], 1);
    let steps: Vec<Value> = events.try_iter().collect();
    assert_eq!(
        steps[7],
        json!({ "host": "web-01", "step": "verify", "state": "failed" })
    );
    assert_eq!(
        steps[9],
        json!({ "host": "web-01", "step": "restore", "state": "done" })
    );
    assert_eq!(steps.len(), 10);

    let asked = net.asked();
    assert_eq!(asked.len(), 4, "{asked:#?}");
    assert!(asked[3].contains(RESTORE), "{}", asked[3]);
    // Password login was never turned off, and the config does not name a
    // key that does not work.
    assert!(!asked.iter().any(|line| line.contains(DISABLE)));
    let config = fs::read_to_string(home.join("config")).unwrap();
    assert!(!config.contains("IdentityFile"), "{config}");
}

#[test]
fn a_config_that_cannot_be_changed_ends_with_the_backup_restored() {
    let (answer, steps, ran) = setup("key-no-disable", host(DISABLE));
    assert_eq!(
        answer.unwrap_err()["chain"][0],
        "cannot turn off password login"
    );
    assert_eq!(
        steps[8..],
        [
            "disable running",
            "disable failed",
            "restore running",
            "restore done"
        ]
    );
    assert_eq!(ran, [BACKUP, INSTALL, " key ", DISABLE, RESTORE]);
}

#[test]
fn a_key_that_stops_working_once_passwords_are_off_brings_them_back() {
    let net = host("");
    let answers = [Ok(()), Err(failed(refused_key()))];
    net.key_logins.lock().unwrap().extend(answers);
    let (answer, steps, ran) = setup("key-locked-out", net);
    assert_eq!(answer.unwrap_err()["class"], "auth");
    assert_eq!(
        steps[8..],
        [
            "disable running",
            "disable failed",
            "restore running",
            "restore done"
        ]
    );
    assert_eq!(ran, [BACKUP, INSTALL, " key ", DISABLE, " key ", RESTORE]);
}

#[test]
fn a_host_that_is_down_fails_before_anything_with_its_class() {
    let mut net = host("");
    net.down = Some(failed(Fail::Refused {
        addr: "10.0.0.5:2222".into(),
    }));
    let (answer, steps, ran) = setup("key-down", net);
    assert_eq!(answer.unwrap_err()["class"], "refused");
    assert_eq!(steps[2..], ["backup running", "backup failed"]);
    assert!(ran.is_empty());
}

#[test]
fn sshd_gets_a_moment_to_come_back_after_its_config_changed() {
    // It reads its config again and is not there for the first knock;
    // that is no reason to undo everything.
    let net = host("");
    let restarting = failed(Fail::Refused {
        addr: "10.0.0.5:2222".into(),
    });
    let answers = [Ok(()), Err(restarting.clone()), Err(restarting), Ok(())];
    net.key_logins.lock().unwrap().extend(answers);
    let (answer, steps, ran) = setup("key-restart", net);
    assert!(answer.is_ok(), "{answer:?}");
    assert_eq!(steps[8..], ["disable running", "disable done"]);
    assert_eq!(
        ran,
        [BACKUP, INSTALL, " key ", DISABLE, " key ", " key ", " key "]
    );
}

#[test]
fn a_backup_that_cannot_be_restored_is_said_out_loud() {
    // Both changing the config and putting it back fail.
    let mut net = host("");
    net.exec = Arc::new(|_, command| {
        let broken = command.contains(DISABLE) || command.contains(RESTORE);
        Ok(match broken {
            true => "tee: /etc/ssh/sshd_config: Read-only file system\n[exit status 1]\n".into(),
            false => "vssh-ok\n".into(),
        })
    });
    let (answer, steps, ran) = setup("key-no-restore", net);
    let err = answer.unwrap_err();
    assert_eq!(
        err["chain"],
        json!([
            "sshd_config could NOT be restored from /etc/ssh/sshd_config.vssh.bak: check it by hand",
            "cannot turn off password login",
            "tee: /etc/ssh/sshd_config: Read-only file system\n[exit status 1]",
        ])
    );
    assert_eq!(steps[10..], ["restore running", "restore failed"]);
    assert_eq!(ran, [BACKUP, INSTALL, " key ", DISABLE, RESTORE]);
}

#[test]
fn key_setup_is_for_a_host_of_the_config() {
    let home = home("key-missing");
    let net = host("");
    let (_app, window) = window(&home, net.clone());
    assert_eq!(
        call(&window, "ssh_key_setup", json!({ "host": "nope" })),
        Err(other("no host named \"nope\""))
    );
    assert!(net.asked().is_empty());
    assert_eq!(fs::read_dir(&home).unwrap().count(), 2, "no key was made");
}
