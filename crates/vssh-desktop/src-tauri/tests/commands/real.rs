//! The real transport against a real sshd, which these tests reconfigure:
//! point them at a throwaway container and never at a machine that matters.
//! Each wants a container of its own, fresh:
//!
//! ```sh
//! docker run -d --name vssh-e2e -p 127.0.0.1:2226:22 alpine:latest sh -c '
//!   apk add --no-cache openssh && ssh-keygen -A && echo root:vssh-e2e | chpasswd &&
//!   echo "PermitRootLogin yes" >> /etc/ssh/sshd_config && exec /usr/sbin/sshd -D -e'
//! VSSH_E2E_SSHD=root@127.0.0.1:2226 VSSH_E2E_PASSWORD=vssh-e2e \
//!   cargo test -p vssh-desktop the_commands_work_on_a_real_sshd -- --ignored
//! ```
//!
//! For `a_failed_key_setup_restores_a_real_sshd` the container must not
//! accept keys: before the `exec`, add
//! `sed -i "s|^AuthorizedKeysFile.*|AuthorizedKeysFile /etc/ssh/none|" /etc/ssh/sshd_config &&`.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use tauri::async_runtime::block_on;
use vssh_core::ssh::Target;
use vssh_core::vault::Vault;
use vssh_desktop::{Backend, Core, Transport};
use zeroize::Zeroizing;

use super::{call, home, listen, next, open, until};

/// A home with one host, `box`, that is the sshd under test, its password
/// in the vault; and the app over it, unlocked. Nothing of the user's own
/// is used to log in: the core keeps known_hosts and looks for keys under
/// $HOME, and would offer the agent's.
fn real(name: &str) -> (PathBuf, Target, String) {
    let spec = std::env::var("VSSH_E2E_SSHD").expect("VSSH_E2E_SSHD=user@host:port");
    let password = std::env::var("VSSH_E2E_PASSWORD").expect("VSSH_E2E_PASSWORD");
    let home = home(name);
    std::env::set_var("HOME", &home);
    std::env::remove_var("SSH_AUTH_SOCK");
    let target = Target::parse(&spec).unwrap();
    let config = format!(
        "Host box\n  HostName {}\n  User {}\n  Port {}\n",
        target.host, target.user, target.port
    );
    fs::write(home.join("config"), config).unwrap();
    let mut vault = Vault::create(&home.join("data/vault"), "master").unwrap();
    vault.set("box", &mut password.clone()).unwrap();
    (home, target, password)
}

#[test]
#[ignore = "reconfigures the sshd it is pointed at: see the top of this file"]
fn a_failed_key_setup_restores_a_real_sshd() {
    let (home, target, password) = real("real-restore");
    let (app, window) = open(Backend::new(
        home.join("config"),
        home.join("data"),
        Core::default(),
    ));
    call(&window, "vault_unlock", json!({ "passphrase": "master" })).unwrap();
    let steps = listen(&app, "key-setup-step");
    let answers = listen(&app, "batch-progress");

    // This sshd reads no authorized_keys: the new key cannot open it.
    let err = call(&window, "ssh_key_setup", json!({ "host": "box" })).unwrap_err();
    assert_eq!(err["class"], "auth", "{err}");
    let heard: Vec<String> = steps
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
        heard[6..],
        [
            "verify running",
            "verify failed",
            "restore running",
            "restore done"
        ]
    );
    // The password still opens the host, and its config is the backup's.
    // (Once sshd is back: putting the backup in place made it start again.)
    let mut no_keys = target;
    no_keys.keys.clear();
    let secret = Some(Zeroizing::new(password));
    until("the password opens the host", || {
        block_on(Core::default().connect(no_keys.clone(), secret.clone())).is_ok()
    });
    let same = "cmp /etc/ssh/sshd_config /etc/ssh/sshd_config.vssh.bak && echo same";
    let batch = json!({ "hosts": ["box"], "command": same });
    call(&window, "snippet_run", batch).unwrap();
    assert_eq!(next(&answers)["output"], "same\n");
}

#[test]
#[ignore = "reconfigures the sshd it is pointed at: see the top of this file"]
fn the_commands_work_on_a_real_sshd() {
    let (home, target, password) = real("real");
    let (app, window) = open(Backend::new(
        home.join("config"),
        home.join("data"),
        Core::default(),
    ));
    let ok =
        |cmd: &str, args: Value| call(&window, cmd, args).unwrap_or_else(|e| panic!("{cmd}: {e}"));

    // Without the vault there is no way in: the failure has its class.
    let locked = call(&window, "sftp_list", json!({ "host": "box", "path": "/" })).unwrap_err();
    assert_eq!(locked["class"], "auth", "{locked}");
    ok("vault_unlock", json!({ "passphrase": "master" }));

    // SFTP: a directory, a file up and down again, renamed, removed.
    let dir = format!("/tmp/vssh-e2e-{}", std::process::id());
    let local = home.join("up.bin");
    let bytes: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    fs::write(&local, &bytes).unwrap();
    ok("sftp_mkdir", json!({ "host": "box", "path": dir }));
    let progress = listen(&app, "sftp-progress:up");
    let remote = format!("{dir}/a.bin");
    ok(
        "sftp_upload",
        json!({ "host": "box", "local": local, "remote": remote, "id": "up" }),
    );
    let steps: Vec<Value> = progress.try_iter().collect();
    assert!(steps.len() > 1, "{steps:?}");
    assert_eq!(
        steps.last().unwrap(),
        &json!({ "done": 300_000, "total": 300_000 })
    );
    let listed = ok("sftp_list", json!({ "host": "box", "path": dir }));
    assert_eq!(listed[0]["name"], "a.bin");
    assert_eq!(listed[0]["size"], 300_000);
    assert_eq!(listed[0]["is_dir"], false);
    assert!(
        listed[0]["mtime"].as_u64().unwrap() > 1_700_000_000,
        "{listed}"
    );
    let renamed = format!("{dir}/b.bin");
    ok(
        "sftp_rename",
        json!({ "host": "box", "from": remote, "to": renamed }),
    );
    let back = home.join("down.bin");
    ok(
        "sftp_download",
        json!({ "host": "box", "remote": renamed, "local": back, "id": "down" }),
    );
    assert_eq!(fs::read(&back).unwrap(), bytes);
    ok(
        "sftp_rm",
        json!({ "host": "box", "path": renamed, "isDir": false }),
    );
    ok(
        "sftp_rm",
        json!({ "host": "box", "path": dir, "isDir": true }),
    );
    let gone = call(&window, "sftp_list", json!({ "host": "box", "path": dir }));
    assert!(gone.is_err(), "{gone:?}");

    // A batch of one.
    let answers = listen(&app, "batch-progress");
    let batch =
        json!({ "hosts": ["box"], "command": "echo {{word}}", "params": { "word": "hello" } });
    ok("snippet_run", batch);
    assert_eq!(next(&answers)["output"], "hello\n");

    // Key setup, every step for real.
    let steps = listen(&app, "key-setup-step");
    let key = ok("ssh_key_setup", json!({ "host": "box" }));
    let heard: Vec<String> = steps
        .try_iter()
        .map(|ev| {
            format!(
                "{} {}",
                ev["step"].as_str().unwrap(),
                ev["state"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(heard.len(), 10, "{heard:?}");
    assert!(
        heard.iter().all(|step| !step.ends_with("failed")),
        "{heard:?}"
    );
    // The password no longer opens the host; the key does, and it is what
    // the config now names, so the commands go on working.
    let mut no_keys = target.clone();
    no_keys.keys.clear();
    let secret = Some(Zeroizing::new(password));
    let refused = block_on(Core::default().connect(no_keys, secret))
        .err()
        .unwrap();
    assert_eq!(serde_json::to_value(refused.class).unwrap(), "auth");
    block_on(Core::default().key_login(target, key.as_str().unwrap().into())).unwrap();
    ok("get_host_stats", json!({ "alias": "box" }));
    // Run again it finds everything in place and leaves one line of each.
    ok("ssh_key_setup", json!({ "host": "box" }));
    let batch = json!({
        "hosts": ["box"],
        "command": "grep -c vssh ~/.ssh/authorized_keys; grep -ciE \"^[[:space:]]*passwordauthentication\" /etc/ssh/sshd_config; head -n1 /etc/ssh/sshd_config",
    });
    ok("snippet_run", batch);
    assert_eq!(
        next(&answers)["output"],
        "1\n1\nPasswordAuthentication no\n"
    );
}
