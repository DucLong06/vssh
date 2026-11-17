//! The vault (D6): unlocked from the window, its passwords then go with
//! every connection to the host they are for. No command hands one back.

use std::fs;
use std::os::unix::fs::PermissionsExt;

use serde_json::{json, Value};
use vssh_core::vault::Vault;

use super::{call, home, listen, next, other, window, Fake};

#[test]
fn the_vault_is_created_then_unlocked_with_its_passphrase() {
    let home = home("vault-unlock");
    let path = home.join("data/vault");
    {
        let (_app, window) = window(&home, Fake::silent());
        let state = || call(&window, "vault_state", json!({}));
        let unlock = |pass: &str| call(&window, "vault_unlock", json!({ "passphrase": pass }));
        assert_eq!(state(), Ok(json!("none")));
        assert_eq!(unlock(""), Err(other("passphrase must not be empty")));
        assert!(!path.exists());

        // There is none yet: the first passphrase makes it.
        assert_eq!(unlock("master"), Ok(Value::Null));
        assert_eq!(state(), Ok(json!("unlocked")));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    // The app again, later: the vault is there and shut.
    let (_app, window) = window(&home, Fake::silent());
    let state = || call(&window, "vault_state", json!({}));
    let unlock = |pass: &str| call(&window, "vault_unlock", json!({ "passphrase": pass }));
    assert_eq!(state(), Ok(json!("locked")));
    assert_eq!(
        unlock("mastre"),
        Err(other("wrong passphrase or damaged vault"))
    );
    assert_eq!(state(), Ok(json!("locked")));
    assert_eq!(unlock("master"), Ok(Value::Null));
    assert_eq!(state(), Ok(json!("unlocked")));
}

#[test]
fn an_unlocked_vault_gives_each_host_its_password() {
    let home = home("vault-passwords");
    let mut vault = Vault::create(&home.join("data/vault"), "master").unwrap();
    vault.set("web-01", &mut "hunter2".to_string()).unwrap();
    drop(vault);
    let net = Fake::silent();
    let (app, window) = window(&home, net.clone());
    let events = listen(&app, "batch-progress");
    let ask = || {
        let stats = |alias: &str| call(&window, "get_host_stats", json!({ "alias": alias }));
        stats("web-01").unwrap();
        stats("bare").unwrap();
        call(
            &window,
            "sftp_list",
            json!({ "host": "web-01", "path": "/" }),
        )
        .unwrap();
        let batch = json!({ "hosts": ["web-01"], "command": "uptime" });
        call(&window, "snippet_run", batch).unwrap();
        next(&events);
        std::mem::take(&mut *net.asked.lock().unwrap())
    };

    let stored = || call(&window, "vault_hosts", json!({}));
    // Locked: keys and the agent only, and nothing is said of what it holds.
    assert_eq!(stored(), Ok(json!([])));
    assert_eq!(
        ask(),
        [
            "web-01 = deploy@10.0.0.5:2222",
            &format!(
                "bare = {}@bare:22",
                std::env::var("USER").unwrap_or("root".into())
            ),
            "deploy@10.0.0.5:2222 List(\"/\")",
            "deploy@10.0.0.5:2222 exec uptime",
        ]
    );
    call(&window, "vault_unlock", json!({ "passphrase": "master" })).unwrap();
    assert_eq!(stored(), Ok(json!(["web-01"])));
    let asked = ask();
    assert_eq!(asked[0], "web-01 = deploy@10.0.0.5:2222 (hunter2)");
    // A host the vault has nothing for gets nothing.
    assert!(!asked[1].contains('('), "{}", asked[1]);
    assert_eq!(asked[2], "deploy@10.0.0.5:2222 (hunter2) List(\"/\")");
    assert_eq!(asked[3], "deploy@10.0.0.5:2222 (hunter2) exec uptime");
}

#[test]
fn unlocking_drops_the_passwords_of_hosts_deleted_while_it_was_locked() {
    let home = home("vault-gone");
    let path = home.join("data/vault");
    let mut vault = Vault::create(&path, "master").unwrap();
    vault.set("web-01", &mut "hunter2".to_string()).unwrap();
    vault.set("bare", &mut "s3cret".to_string()).unwrap();
    drop(vault);
    let (_app, window) = window(&home, Fake::silent());
    let file = || Vault::open(&path, "master").unwrap();

    // Deleted with the vault shut: the host goes, its password cannot.
    call(&window, "hosts_delete", json!({ "alias": "bare" })).unwrap();
    assert_eq!(file().get("bare"), Some("s3cret"));

    // Unlocked, the vault lets go of what no host is there for.
    call(&window, "vault_unlock", json!({ "passphrase": "master" })).unwrap();
    assert_eq!(
        call(&window, "vault_hosts", json!({})),
        Ok(json!(["web-01"]))
    );
    assert_eq!(
        (file().get("web-01"), file().get("bare")),
        (Some("hunter2"), None)
    );
}

#[test]
fn a_new_passphrase_drops_the_passwords_of_hosts_that_are_gone_too() {
    let home = home("vault-gone-change");
    let path = home.join("data/vault");
    let mut vault = Vault::create(&path, "master").unwrap();
    vault.set("web-01", &mut "hunter2".to_string()).unwrap();
    vault.set("gone-box", &mut "s3cret".to_string()).unwrap();
    drop(vault);
    let (_app, window) = window(&home, Fake::silent());

    let change = json!({ "old": "master", "new": "battery staple" });
    call(&window, "vault_pass_change", change).unwrap();
    let file = Vault::open(&path, "battery staple").unwrap();
    assert_eq!(file.names().collect::<Vec<_>>(), ["web-01"]);
}

#[test]
fn a_config_that_is_not_there_costs_the_vault_nothing() {
    let home = home("vault-no-config");
    let path = home.join("data/vault");
    let mut vault = Vault::create(&path, "master").unwrap();
    vault.set("web-01", &mut "hunter2".to_string()).unwrap();
    drop(vault);
    // Moved away, or another HOME: no host is listed, and none was deleted.
    fs::remove_file(home.join("config")).unwrap();
    let (_app, window) = window(&home, Fake::silent());

    call(&window, "vault_unlock", json!({ "passphrase": "master" })).unwrap();
    assert_eq!(
        call(&window, "vault_hosts", json!({})),
        Ok(json!(["web-01"]))
    );
    assert_eq!(
        Vault::open(&path, "master").unwrap().get("web-01"),
        Some("hunter2")
    );
}

#[test]
fn the_passphrase_is_changed_by_who_knows_the_old_one() {
    let home = home("vault-change");
    let path = home.join("data/vault");
    let mut vault = Vault::create(&path, "master").unwrap();
    vault.set("web-01", &mut "hunter2".to_string()).unwrap();
    drop(vault);
    let (_app, window) = window(&home, Fake::silent());
    let change = |old: &str, new: &str| {
        call(
            &window,
            "vault_pass_change",
            json!({ "old": old, "new": new }),
        )
    };

    assert_eq!(
        change("guess", "mine now"),
        Err(other("wrong passphrase or damaged vault"))
    );
    assert_eq!(
        change("master", ""),
        Err(other("passphrase must not be empty"))
    );
    assert!(Vault::open(&path, "master").is_ok());

    // It need not be unlocked for that, and is afterwards.
    assert_eq!(call(&window, "vault_state", json!({})), Ok(json!("locked")));
    assert_eq!(change("master", "battery staple"), Ok(Value::Null));
    assert_eq!(
        call(&window, "vault_state", json!({})),
        Ok(json!("unlocked"))
    );
    assert!(Vault::open(&path, "master").is_err());
    assert_eq!(
        Vault::open(&path, "battery staple").unwrap().get("web-01"),
        Some("hunter2")
    );
}

#[test]
fn an_export_is_a_sealed_copy_in_a_new_file() {
    let home = home("vault-export");
    let mut vault = Vault::create(&home.join("data/vault"), "master").unwrap();
    vault.set("web-01", &mut "hunter2".to_string()).unwrap();
    drop(vault);
    let (_app, window) = window(&home, Fake::silent());
    let copy = home.join("backup.vault");
    let export = || call(&window, "vault_export", json!({ "path": copy }));

    assert_eq!(export(), Err(other("the vault is locked")));
    assert!(!copy.exists());
    call(&window, "vault_unlock", json!({ "passphrase": "master" })).unwrap();
    assert_eq!(export(), Ok(Value::Null));
    // Nothing readable in it, and only its passphrase opens it.
    let sealed = fs::read(&copy).unwrap();
    assert!(!sealed.windows(7).any(|bytes| bytes == b"hunter2"));
    assert_eq!(
        fs::metadata(&copy).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        Vault::open(&copy, "master").unwrap().get("web-01"),
        Some("hunter2")
    );
    let err = export().unwrap_err();
    assert!(
        err["chain"][0]
            .as_str()
            .unwrap()
            .ends_with("already exists"),
        "{err}"
    );
}
