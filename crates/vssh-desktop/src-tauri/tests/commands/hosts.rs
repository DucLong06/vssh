//! Hosts (D6): added, changed and removed in the ssh config and the
//! database together, by the rules of the TUI's form.

use std::fs;
use std::sync::atomic::Ordering;

use serde_json::{json, Value};
use vssh_core::ssh::{Target, Via};
use vssh_core::store::db::{Meta, Snippet};
use vssh_core::store::ssh_config::SshConfig;
use vssh_core::vault::Vault;

use super::{call, db, home, listen, next, other, until, window, Fake, SSH_CONFIG};

fn form(alias: &str) -> Value {
    json!({
        "alias": alias, "hostname": "10.0.0.7", "user": "ops", "port": 2200,
        "identity_file": "~/.ssh/db", "grp": "prod", "tags": "db eu", "notes": "primary",
    })
}

#[test]
fn a_host_is_added_to_the_config_and_the_database() {
    let home = home("hosts-add");
    let (_app, window) = window(&home, Fake::silent());
    let added = call(&window, "hosts_add", json!({ "form": form(" db.lan ") }));
    let host = json!({
        "alias": "db.lan", "hostname": "10.0.0.7", "user": "ops", "port": 2200,
        "meta": {
            "grp": "prod", "tags": "db eu", "notes": "primary", "pinned": false,
            "last_connected": 0, "connects": 0, "paused": false,
        },
    });
    assert_eq!(added, Ok(host.clone()));
    assert_eq!(call(&window, "list_hosts", json!({})).unwrap()[2], host);
    // Its block goes ahead of the pattern that would otherwise speak for it.
    let config = fs::read_to_string(home.join("config")).unwrap();
    let block = "Host db.lan\n    HostName 10.0.0.7\n    User ops\n    Port 2200\n    \
                 IdentityFile ~/.ssh/db\n\nHost *.lan\n  User ops\n";
    let before = SSH_CONFIG.trim_end_matches("Host *.lan\n  User ops\n");
    assert_eq!(config, format!("{before}{block}"));

    // Only the alias is required; what is left out is left to `ssh`.
    let bare = call(
        &window,
        "hosts_add",
        json!({ "form": { "alias": "plain" } }),
    )
    .unwrap();
    assert_eq!(bare["hostname"], Value::Null);
    assert_eq!(bare["port"], Value::Null);
    let config = fs::read_to_string(home.join("config")).unwrap();
    assert!(config.ends_with("  User ops\n\nHost plain\n"), "{config}");
}

#[test]
fn a_host_form_is_checked_as_in_the_tui() {
    let home = home("hosts-bad");
    let (_app, window) = window(&home, Fake::silent());
    let add = |alias: &str| call(&window, "hosts_add", json!({ "form": form(alias) }));
    let word = other("alias must be one word without wildcards");
    for alias in [
        "",
        "  ",
        "two words",
        "web-*",
        "a?b",
        "!x",
        "a#b",
        "a=b",
        "a\"b",
    ] {
        assert_eq!(add(alias), Err(word.clone()), "{alias:?}");
    }
    assert_eq!(add("web-01"), Err(other("host web-01 already exists")));
    // A port that is no port never reaches the command.
    let mut far = form("far");
    far["port"] = json!(70000);
    assert!(call(&window, "hosts_add", json!({ "form": far }))
        .unwrap_err()
        .is_string());
    // A line break cannot carry a second option into the config.
    let mut sly = form("sly");
    sly["hostname"] = json!("10.0.0.8\n  ProxyCommand reboot");
    assert!(call(&window, "hosts_add", json!({ "form": sly })).is_ok());
    let config = fs::read_to_string(home.join("config")).unwrap();
    assert!(
        config.contains("    HostName 10.0.0.8   ProxyCommand reboot\n"),
        "{config}"
    );
    assert_eq!(config.matches("Host ").count(), 4, "{config}");
}

#[test]
fn a_host_is_changed_and_renamed_with_everything_it_has() {
    let home = home("hosts-update");
    let db = db(&home);
    let pinned = Meta {
        pinned: true,
        ..Meta::default()
    };
    db.set_meta("web-01", &pinned).unwrap();
    db.touch("web-01").unwrap();
    let logs = Snippet {
        id: 0,
        host: "web-01".into(),
        name: "logs".into(),
        command: "journalctl -f".into(),
    };
    db.save_snippet(&logs).unwrap();
    let net = Fake::silent();
    let (app, window) = window(&home, net.clone());
    let events = listen(&app, "forward-state");
    let update = |alias: &str, form: Value| {
        call(
            &window,
            "hosts_update",
            json!({ "alias": alias, "form": form }),
        )
    };
    let forward = json!({ "host": "web-01", "kind": "D", "localPort": 1080 });
    call(&window, "forward_add", forward).unwrap();
    let toggle = json!({ "host": "web-01", "kind": "D", "value": "1080" });
    call(&window, "forward_toggle", toggle).unwrap();
    next(&events);

    // Same name: fields change, an emptied one goes, the rest of the block stays.
    let changed = json!({ "alias": "web-01", "hostname": "10.0.0.6", "user": "", "tags": "web" });
    let host = update("web-01", changed).unwrap();
    assert_eq!(
        (&host["hostname"], &host["user"]),
        (&json!("10.0.0.6"), &Value::Null)
    );
    assert_eq!(host["port"], Value::Null, "the form is the whole host");
    // What only the app knows is kept: the pin, and how often it was used.
    assert_eq!(host["meta"]["pinned"], true);
    assert_eq!(host["meta"]["connects"], 1);
    assert_eq!(host["meta"]["tags"], "web");
    let config = fs::read_to_string(home.join("config")).unwrap();
    let block = "Host web-01\n    HostName 10.0.0.6\n    DynamicForward 1080\n\nHost bare\n";
    assert!(config.starts_with(block), "{config}");
    assert!(!net.dropped.load(Ordering::SeqCst), "its forward runs on");

    // Renamed: the block, the metadata and the snippets follow; a forward
    // that ran under the old name is stopped.
    let host = update("web-01", form("web-02")).unwrap();
    assert_eq!(host["alias"], "web-02");
    assert_eq!(host["meta"]["pinned"], true);
    let config = fs::read_to_string(home.join("config")).unwrap();
    assert!(
        config.starts_with("Host web-02\n    HostName 10.0.0.7\n"),
        "{config}"
    );
    assert_eq!(
        call(&window, "snippets_list", json!({})).unwrap()[0]["host"],
        "web-02"
    );
    until("the forward is stopped", || {
        net.dropped.load(Ordering::SeqCst)
    });

    assert_eq!(
        update("web-01", form("x")),
        Err(other("no host named \"web-01\""))
    );
    assert_eq!(
        update("web-02", form("bare")),
        Err(other("host bare already exists"))
    );
}

#[test]
fn a_host_is_deleted_with_everything_it_has() {
    let home = home("hosts-delete");
    let logs = Snippet {
        id: 0,
        host: "web-01".into(),
        name: "logs".into(),
        command: "journalctl -f".into(),
    };
    db(&home).save_snippet(&logs).unwrap();
    let net = Fake::silent();
    let (app, window) = window(&home, net.clone());
    let events = listen(&app, "forward-state");
    let forward = json!({ "host": "web-01", "kind": "D", "localPort": 1080 });
    call(&window, "forward_add", forward).unwrap();
    let toggle = json!({ "host": "web-01", "kind": "D", "value": "1080" });
    call(&window, "forward_toggle", toggle).unwrap();
    next(&events);

    let delete = || call(&window, "hosts_delete", json!({ "alias": "web-01" }));
    assert_eq!(delete(), Ok(Value::Null));
    let config = fs::read_to_string(home.join("config")).unwrap();
    assert_eq!(config, "Host bare\n\nHost *.lan\n  User ops\n");
    let hosts = call(&window, "list_hosts", json!({})).unwrap();
    assert_eq!(hosts.as_array().unwrap().len(), 1);
    assert!(!db(&home).metas().unwrap().contains_key("web-01"));
    assert_eq!(call(&window, "snippets_list", json!({})), Ok(json!([])));
    until("the forward is stopped", || {
        net.dropped.load(Ordering::SeqCst)
    });
    assert_eq!(delete(), Err(other("no host named \"web-01\"")));
}

#[test]
fn hosts_of_another_ssh_config_are_imported_with_their_jump_hosts() {
    let home = home("hosts-import");
    let theirs = home.join("their_config");
    fs::write(
        &theirs,
        "Host bastion\n  HostName 203.0.113.1\n  User jump\n\n\
         Host inner\n  HostName 10.0.0.5\n  ProxyJump bastion\n\n\
         Host web-01\n  HostName elsewhere\n\n\
         Host *\n  User ops\n",
    )
    .unwrap();
    let (_app, window) = window(&home, Fake::silent());
    let import =
        |path: &std::path::Path| call(&window, "hosts_import_ssh_config", json!({ "path": path }));

    assert_eq!(import(&theirs), Ok(json!(["bastion", "inner"])));
    let hosts = call(&window, "list_hosts", json!({})).unwrap();
    let aliases: Vec<&str> = hosts
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["alias"].as_str().unwrap())
        .collect();
    assert_eq!(aliases, ["web-01", "bare", "bastion", "inner"]);
    // The host we had is ours still; the new ones have their rows.
    assert_eq!(hosts[0]["hostname"], "10.0.0.5");
    assert_eq!(db(&home).metas().unwrap().len(), 4);
    // `inner` is reached as it was there: through the bastion, as the
    // user their `Host *` gave it.
    let config = SshConfig::load(&home.join("config")).unwrap();
    let inner = Target::from_config(&config, "inner").unwrap();
    assert_eq!(inner.user, "ops");
    let Via::Jump(hops) = inner.via else {
        panic!("inner has a jump host");
    };
    assert_eq!(
        (hops[0].host.as_str(), hops[0].user.as_str()),
        ("203.0.113.1", "jump")
    );

    // Again: nothing new, and the file is not written.
    let before = fs::read_to_string(home.join("config")).unwrap();
    assert_eq!(import(&theirs), Ok(json!([])));
    assert_eq!(fs::read_to_string(home.join("config")).unwrap(), before);
    let err = import(&home.join("nowhere")).unwrap_err();
    assert!(
        err["chain"][0]
            .as_str()
            .unwrap()
            .starts_with("cannot read "),
        "{err}"
    );
}

#[test]
fn a_quick_connect_is_saved_as_a_host_and_its_password_only_in_the_vault() {
    let home = home("hosts-quick");
    let (_app, window) = window(&home, Fake::silent());
    let save = |host: &str, password: Value| {
        call(
            &window,
            "host_quick_save",
            json!({ "host": host, "password": password }),
        )
    };

    assert_eq!(
        save("deploy@10.0.0.9:2200", Value::Null),
        Ok(json!("10.0.0.9"))
    );
    let config = fs::read_to_string(home.join("config")).unwrap();
    assert!(
        config.contains("Host 10.0.0.9\n    HostName 10.0.0.9\n    User deploy\n    Port 2200\n")
    );
    assert!(db(&home).metas().unwrap().contains_key("10.0.0.9"));
    // The same address again gets the longer name.
    assert_eq!(
        save("deploy@10.0.0.9:2200", Value::Null),
        Ok(json!("deploy@10.0.0.9"))
    );
    assert_eq!(
        save("not a host", Value::Null),
        Err(other("expected user@host or user@host:port"))
    );

    // A password needs somewhere to go before anything is written.
    let before = fs::read_to_string(home.join("config")).unwrap();
    assert_eq!(
        save("root@db.example", json!("s3cret")),
        Err(other("the vault is locked"))
    );
    assert_eq!(fs::read_to_string(home.join("config")).unwrap(), before);
    call(&window, "vault_unlock", json!({ "passphrase": "master" })).unwrap();
    assert_eq!(
        save("root@db.example", json!("s3cret")),
        Ok(json!("db.example"))
    );
    let config = fs::read_to_string(home.join("config")).unwrap();
    assert!(!config.contains("s3cret"));
    let vault = Vault::open(&home.join("data/vault"), "master").unwrap();
    assert_eq!(vault.get("db.example"), Some("s3cret"));
}

/// D14: the road to a host is its `ProxyJump`, hop by hop, as ssh writes it;
/// the key a hop was given is the app's to keep, and the one the hop is
/// logged in to with.
#[test]
fn a_hop_of_the_road_is_given_a_key_of_its_own_and_the_config_says_proxyjump_only() {
    let home = home("hosts-hops");
    let (_app, window) = window(&home, Fake::silent());
    let read = || call(&window, "hosts_form", json!({ "alias": "web-01" })).unwrap();
    let update = |form: &Value| {
        let args = json!({ "alias": "web-01", "form": form });
        call(&window, "hosts_update", args).unwrap()
    };
    let config = || fs::read_to_string(home.join("config")).unwrap();

    let mut form = read();
    form["proxy_jump"] = json!(" bare , ops@gw:2222 ");
    form["jump_auth"] = json!([
        { "hop": "ops@gw:2222", "key": " ~/.ssh/gw ", "user": "jump" },
        { "hop": "bare", "key": "~/.ssh/hop" },
        // No hop of this road, and a hop with no key: nothing to keep.
        { "hop": "gone", "key": "~/.ssh/none" },
        { "hop": "bare", "key": " " },
    ]);
    update(&form);
    let written = config();
    assert!(
        written.contains("    ProxyJump bare,ops@gw:2222\n"),
        "{written}"
    );
    assert!(!written.contains(".ssh/hop") && !written.contains(".ssh/gw"));
    // Read back in the order of the road.
    let form = read();
    assert_eq!(form["proxy_jump"], "bare,ops@gw:2222");
    assert_eq!(
        form["jump_auth"],
        json!([
            { "hop": "bare", "key": "~/.ssh/hop", "user": "" },
            { "hop": "ops@gw:2222", "key": "~/.ssh/gw", "user": "jump" },
        ])
    );
    // What a connection is made with: each hop's own key and no other.
    let hops = || {
        let sshc = SshConfig::load(&home.join("config")).unwrap();
        let given = db(&home).jump_auth().unwrap();
        match Target::with_jump_auth(&sshc, "web-01", &given).unwrap().via {
            Via::Jump(hops) => hops,
            other => panic!("{other:?}"),
        }
    };
    let road = hops();
    assert_eq!(road.len(), 2);
    assert!(road[0].only_keys && road[0].keys[0].ends_with(".ssh/hop"));
    assert!(road[1].only_keys && road[1].keys[0].ends_with(".ssh/gw"));
    assert_eq!((road[1].user.as_str(), road[1].port), ("jump", 2222));

    // A form that says nothing of the keys leaves them; one that says none takes them.
    let mut silent = read();
    silent.as_object_mut().unwrap().remove("jump_auth");
    update(&silent);
    assert_eq!(read()["jump_auth"].as_array().unwrap().len(), 2);
    // A hop taken off the road takes its key with it.
    let mut shorter = read();
    shorter["proxy_jump"] = json!("bare");
    update(&shorter);
    assert_eq!(
        read()["jump_auth"],
        json!([{ "hop": "bare", "key": "~/.ssh/hop", "user": "" }])
    );
    assert_eq!(db(&home).jump_auth().unwrap().len(), 1);
    let mut plain = read();
    plain["jump_auth"] = json!([]);
    update(&plain);
    assert!(db(&home).jump_auth().unwrap().is_empty());
    assert!(!hops()[0].only_keys);
}

#[test]
fn the_window_form_adds_the_jump_host_the_pin_and_the_vault_password() {
    let home = home("hosts-form");
    let (_app, window) = window(&home, Fake::silent());
    let read = |alias: &str| call(&window, "hosts_form", json!({ "alias": alias }));
    let update = |alias: &str, form: Value| {
        call(
            &window,
            "hosts_update",
            json!({ "alias": alias, "form": form }),
        )
    };

    // What the host's own block says, and nothing a pattern adds to it.
    assert_eq!(
        read("web-01"),
        Ok(json!({
            "alias": "web-01", "hostname": "10.0.0.5", "user": "deploy", "port": 2222,
            "identity_file": "", "proxy_jump": "", "grp": "", "tags": "", "notes": "",
            "pinned": false, "jump_auth": [],
        }))
    );
    assert_eq!(read("nobody"), Err(other("no host named \"nobody\"")));

    // A password needs the vault open, before anything is written.
    let mut form = read("web-01").unwrap();
    form["proxy_jump"] = json!("bare");
    form["pinned"] = json!(true);
    form["password"] = json!("hunter2");
    let before = fs::read_to_string(home.join("config")).unwrap();
    assert_eq!(
        update("web-01", form.clone()),
        Err(other("the vault is locked"))
    );
    assert_eq!(fs::read_to_string(home.join("config")).unwrap(), before);

    call(&window, "vault_unlock", json!({ "passphrase": "master" })).unwrap();
    let host = update("web-01", form).unwrap();
    assert_eq!(host["meta"]["pinned"], true);
    let config = fs::read_to_string(home.join("config")).unwrap();
    assert!(config.contains("    ProxyJump bare\n"), "{config}");
    assert!(!config.contains("hunter2"));
    let stored = || call(&window, "vault_hosts", json!({})).unwrap();
    assert_eq!(stored(), json!(["web-01"]));
    // What was saved is what the form reads back, the password apart.
    let form = read("web-01").unwrap();
    assert_eq!(
        (&form["proxy_jump"], &form["pinned"]),
        (&json!("bare"), &json!(true))
    );
    assert_eq!(form.get("password"), None);

    // A form that says nothing of them leaves all three as they are; the
    // password follows the host to its new name.
    let mut renamed = form.clone();
    renamed["alias"] = json!("web-09");
    for key in ["proxy_jump", "pinned"] {
        renamed.as_object_mut().unwrap().remove(key);
    }
    let host = update("web-01", renamed).unwrap();
    assert_eq!(host["meta"]["pinned"], true);
    assert_eq!(read("web-09").unwrap()["proxy_jump"], "bare");
    assert_eq!(stored(), json!(["web-09"]));
    let vault = || Vault::open(&home.join("data/vault"), "master").unwrap();
    assert_eq!(vault().get("web-09"), Some("hunter2"));

    // Emptied: the jump host goes, the pin, and the stored password.
    let mut plain = read("web-09").unwrap();
    plain["proxy_jump"] = json!("");
    plain["pinned"] = json!(false);
    plain["password"] = json!("");
    let host = update("web-09", plain).unwrap();
    assert_eq!(host["meta"]["pinned"], false);
    let config = fs::read_to_string(home.join("config")).unwrap();
    assert!(!config.contains("ProxyJump"), "{config}");
    assert_eq!(stored(), json!([]));

    // A deleted host's password does not wait for the next host of that name.
    let mut again = read("bare").unwrap();
    again["password"] = json!("s3cret");
    update("bare", again).unwrap();
    assert_eq!(vault().get("bare"), Some("s3cret"));
    call(&window, "hosts_delete", json!({ "alias": "bare" })).unwrap();
    assert_eq!(vault().get("bare"), None);
}

#[test]
fn an_import_is_previewed_and_takes_only_the_hosts_chosen() {
    let home = home("hosts-preview");
    let theirs = home.join("their_config");
    fs::write(
        &theirs,
        "Host bastion\n  HostName 203.0.113.1\n  User jump\n\n\
         Host inner\n  HostName 10.0.0.5\n  ProxyJump bastion\n\n\
         Host web-01\n  HostName elsewhere\n\n\
         Host *\n  User ops\n  Port 2200\n",
    )
    .unwrap();
    let (_app, window) = window(&home, Fake::silent());
    let preview = || call(&window, "hosts_import_preview", json!({ "path": theirs }));
    let meta = json!({
        "grp": "", "tags": "", "notes": "", "pinned": false, "last_connected": 0, "connects": 0,
        "paused": false,
    });

    // Those we have not: as they are reached there, patterns and all.
    let before = fs::read_to_string(home.join("config")).unwrap();
    assert_eq!(
        preview(),
        Ok(json!([
            { "alias": "bastion", "hostname": "203.0.113.1", "user": "jump", "port": 2200, "meta": meta },
            { "alias": "inner", "hostname": "10.0.0.5", "user": "ops", "port": 2200, "meta": meta },
        ]))
    );
    assert_eq!(fs::read_to_string(home.join("config")).unwrap(), before);

    // Only what was ticked comes over, and a host we have is never a choice.
    let only = json!({ "path": theirs, "only": ["inner", "web-01"] });
    assert_eq!(
        call(&window, "hosts_import_ssh_config", only),
        Ok(json!(["inner"]))
    );
    let hosts = call(&window, "list_hosts", json!({})).unwrap();
    assert_eq!(hosts[0]["hostname"], "10.0.0.5");
    assert_eq!(hosts.as_array().unwrap().len(), 3);
    assert_eq!(preview().unwrap().as_array().unwrap().len(), 1);

    let err = call(
        &window,
        "hosts_import_preview",
        json!({ "path": "~/no/such/config" }),
    )
    .unwrap_err();
    let words = err["chain"][0].as_str().unwrap();
    // The `~` is the user's home, not a directory of that name.
    assert!(
        words.starts_with("cannot read /") && !words.contains('~'),
        "{err}"
    );
}

#[test]
fn the_hosts_go_out_as_json_without_a_password_and_come_in_again() {
    let home = home("hosts-json");
    let (_app, window) = window(&home, Fake::silent());
    // A host with everything a form has, and a password in the vault.
    call(&window, "vault_unlock", json!({ "passphrase": "open" })).unwrap();
    let mut full = form("web-01");
    full["proxy_jump"] = json!("bare");
    full["pinned"] = json!(true);
    full["password"] = json!("s3cret-of-web-01");
    let update = json!({ "alias": "web-01", "form": full });
    call(&window, "hosts_update", update).unwrap();

    let file = home.join("hosts.json");
    let export = || call(&window, "export_hosts_json", json!({ "path": file }));
    assert_eq!(export(), Ok(json!(2)));
    let text = fs::read_to_string(&file).unwrap();
    let web = json!({
        "alias": "web-01", "hostname": "10.0.0.7", "user": "ops", "port": 2200,
        "identity_file": "~/.ssh/db", "proxy_jump": "bare", "grp": "prod",
        "tags": "db eu", "notes": "primary", "pinned": true, "jump_auth": [],
    });
    let bare = json!({
        "alias": "bare", "hostname": "", "user": "", "port": null, "identity_file": "",
        "proxy_jump": "", "grp": "", "tags": "", "notes": "", "pinned": false,
        "jump_auth": [],
    });
    let written: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(written, json!([web, bare]));
    // For a person to read, and nothing of the vault in it.
    assert!(
        text.starts_with("[\n  {\n    \"alias\": \"web-01\",\n"),
        "{text}"
    );
    assert!(
        !text.contains("password") && !text.contains("s3cret"),
        "{text}"
    );
    // A file that is there is not written over.
    let again = export().unwrap_err();
    let words = again["chain"][0].as_str().unwrap();
    assert!(words.starts_with("cannot write "), "{again}");
    assert_eq!(fs::read_to_string(&file).unwrap(), text);

    // Another machine: it has web-01 as it first was, and `bare`.
    let there = super::home("hosts-json-there");
    let (_app, window) = super::window(&there, Fake::silent());
    let config = || fs::read_to_string(there.join("config")).unwrap();
    let preview = call(
        &window,
        "import_hosts_json_preview",
        json!({ "path": file }),
    )
    .unwrap();
    let taken: Vec<_> = preview
        .as_array()
        .unwrap()
        .iter()
        .map(|h| &h["taken"])
        .collect();
    assert_eq!(taken, [true, true]);
    assert_eq!(preview[0]["alias"], "web-01");
    assert_eq!(preview[0]["hostname"], "10.0.0.7");
    assert_eq!(config(), SSH_CONFIG, "a preview writes nothing");

    // One more host in the file, which is new there.
    let mut hosts = written.clone();
    hosts
        .as_array_mut()
        .unwrap()
        .push(json!({ "alias": "db", "hostname": "10.0.0.9", "tags": "db" }));
    fs::write(&file, hosts.to_string()).unwrap();
    let import = |only: Value, overwrite: bool| {
        let args = json!({ "path": file, "only": only, "overwrite": overwrite });
        call(&window, "import_hosts_json", args)
    };
    // Skipped: the hosts it has are left as they are, the new one comes in.
    assert_eq!(
        import(json!(["web-01", "bare", "db"]), false),
        Ok(json!(["db"]))
    );
    let listed = call(&window, "list_hosts", json!({})).unwrap();
    assert_eq!(listed[0]["hostname"], "10.0.0.5");
    assert_eq!(listed[2]["alias"], "db");
    assert_eq!(listed[2]["meta"]["tags"], "db");
    // Written over, and only the one that was chosen.
    assert_eq!(import(json!(["web-01"]), true), Ok(json!(["web-01"])));
    let listed = call(&window, "list_hosts", json!({})).unwrap();
    let meta = json!({
        "grp": "prod", "tags": "db eu", "notes": "primary", "pinned": true,
        "last_connected": 0, "connects": 0, "paused": false,
    });
    let host = json!({ "alias": "web-01", "hostname": "10.0.0.7", "user": "ops", "port": 2200, "meta": meta });
    assert_eq!(listed[0], host);
    let sshc = SshConfig::load(&there.join("config")).unwrap();
    assert_eq!(sshc.own("web-01", "ProxyJump"), "bare");
    assert_eq!(sshc.own("web-01", "IdentityFile"), "~/.ssh/db");
    assert_eq!(listed.as_array().unwrap().len(), 3);
}

#[test]
fn a_hosts_file_is_checked_before_any_of_it_is_written() {
    let home = home("hosts-json-bad");
    let (_app, window) = window(&home, Fake::silent());
    let file = home.join("hosts.json");
    let said = |text: &str| -> String {
        fs::write(&file, text).unwrap();
        let all = json!(["fine", "web-01", "two words"]);
        let args = json!({ "path": file, "only": all, "overwrite": true });
        let err = call(&window, "import_hosts_json", args).unwrap_err();
        let preview = call(
            &window,
            "import_hosts_json_preview",
            json!({ "path": file }),
        );
        assert_eq!(preview, Err(err.clone()), "the preview says the same");
        let config = fs::read_to_string(home.join("config")).unwrap();
        assert_eq!(config, SSH_CONFIG, "{text} was written");
        err["chain"][0].as_str().unwrap().to_string()
    };
    let fine = r#"{ "alias": "fine", "hostname": "10.0.0.1" }"#;
    for not_one in [
        "Host web-01\n  HostName 10.0.0.5\n".to_string(),
        r#"{ "alias": "fine" }"#.to_string(),
        format!(r#"[{fine}, {{ "alias": "x", "shell": "sh" }}]"#),
        format!(r#"[{fine}, {{ "alias": "x", "port": 70000 }}]"#),
    ] {
        assert!(
            said(&not_one).ends_with(" is not a hosts file"),
            "{not_one}"
        );
    }
    let word = "alias must be one word without wildcards";
    assert_eq!(
        said(&format!(r#"[{fine}, {{ "alias": "two words" }}]"#)),
        word
    );
    assert_eq!(said(&format!(r#"[{fine}, {{ "alias": "web-*" }}]"#)), word);
    assert_eq!(
        said(&format!(r#"[{fine}, {{ "hostname": "10.0.0.2" }}]"#)),
        word
    );
    let twice = said(&format!(r#"[{fine}, {{ "alias": " fine " }}]"#));
    assert_eq!(twice, "host fine is in the file twice");
    let secret = said(&format!(r#"[{fine}, {{ "alias": "x", "password": "p" }}]"#));
    assert_eq!(secret, "a hosts file holds no passwords");
    fs::remove_file(&file).unwrap();
    let args = json!({ "path": file, "only": [], "overwrite": false });
    let gone = call(&window, "import_hosts_json", args).unwrap_err();
    assert!(
        gone["chain"][0]
            .as_str()
            .unwrap()
            .starts_with("cannot read "),
        "{gone}"
    );

    // A line break in a value stays in that value: no directive of its own.
    fs::write(
        &file,
        r#"[{ "alias": "sly", "hostname": "h\n  ProxyCommand touch /tmp/x" }]"#,
    )
    .unwrap();
    let args = json!({ "path": file, "only": ["sly"], "overwrite": false });
    assert_eq!(call(&window, "import_hosts_json", args), Ok(json!(["sly"])));
    let sshc = SshConfig::load(&home.join("config")).unwrap();
    assert_eq!(sshc.own("sly", "ProxyCommand"), "");
    assert_eq!(sshc.own("sly", "HostName"), "h   ProxyCommand touch /tmp/x");
}
