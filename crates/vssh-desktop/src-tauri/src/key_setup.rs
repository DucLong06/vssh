//! SSH key setup for the window (D6): a new Ed25519 key for a host, added
//! to its `authorized_keys`, proven to open it, and only then password
//! login is turned off. `sshd_config` is backed up before anything on the
//! host changes and put back if a later step fails. The window hears of
//! each step on `key-setup-step`.

use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime, State};
use vssh_core::conn_err::Class;
use vssh_core::ssh::{generate_key, Target};

use crate::{Backend, Password, Remote, Reply};

const CONFIG: &str = "/etc/ssh/sshd_config";

/// Root, or sudo if it asks no password. `sshd` is not on every PATH.
const ROOT: &str =
    r#"PATH="$PATH:/usr/sbin:/sbin"; if [ "$(id -u)" = 0 ]; then S=; else S="sudo -n"; fi"#;

/// A running sshd reads its config again on SIGHUP; one started per
/// connection by systemd has no pid file and needs no telling.
const RELOAD: &str = r#"pid=$(cat /run/sshd.pid 2>/dev/null || cat /var/run/sshd.pid 2>/dev/null || true); if [ -n "$pid" ]; then $S kill -HUP "$pid"; else $S systemctl reload ssh 2>/dev/null || $S systemctl reload sshd 2>/dev/null || true; fi"#;

fn backup() -> String {
    format!("{ROOT}; $S cp -p {CONFIG} {CONFIG}.vssh.bak")
}

/// Appends the public line unless it is there, on a line of its own.
fn install(public: &str) -> String {
    let keys = "~/.ssh/authorized_keys";
    format!(
        "umask 077; mkdir -p ~/.ssh; touch {keys}; \
         [ -z \"$(tail -c1 {keys})\" ] || echo >> {keys}; \
         grep -qxF \"{public}\" {keys} || echo \"{public}\" >> {keys}"
    )
}

/// `PasswordAuthentication no` as the first line, where no `Include` and no
/// later line can speak before it; sshd must accept the file and read it so.
// shortcut: a `Match` block that turns passwords back on for some users is
// left as it is; take those lines out too if such a host shows up.
fn disable() -> String {
    format!(
        "{ROOT}; {{ echo \"PasswordAuthentication no\"; \
         grep -viE \"^[[:space:]]*PasswordAuthentication[[:space:]=]\" {CONFIG}.vssh.bak || true; }} \
         | $S tee {CONFIG} >/dev/null; \
         $S sshd -t; $S sshd -T | grep -qix \"passwordauthentication no\"; {RELOAD}"
    )
}

fn restore() -> String {
    format!("{ROOT}; $S cp -p {CONFIG}.vssh.bak {CONFIG}; {RELOAD}")
}

/// What `key-setup-step` carries. Steps: `generate`, `backup`, `install`,
/// `verify`, `disable`, and `restore` after a failure; each is `running`,
/// then `done` or `failed`.
#[derive(Clone, Serialize)]
struct Step<'a> {
    host: &'a str,
    step: &'static str,
    state: &'static str,
}

struct Setup<'a, R: Runtime> {
    app: &'a AppHandle<R>,
    backend: &'a Backend,
    host: &'a str,
    target: Target,
    password: Password,
    key: PathBuf,
}

impl<R: Runtime> Setup<'_, R> {
    /// Does one step, telling the window when it starts and how it ended.
    async fn step<T>(&self, step: &'static str, work: impl Future<Output = Reply<T>>) -> Reply<T> {
        let host = self.host;
        let say = |state| drop(self.app.emit("key-setup-step", Step { host, step, state }));
        say("running");
        let res = work.await;
        say(if res.is_ok() { "done" } else { "failed" });
        res
    }

    /// The public line of the host's key, which is made if there is none.
    fn public(&self) -> Result<String> {
        if !self.key.exists() {
            return generate_key(&self.key, "vssh");
        }
        let public = format!("{}.pub", self.key.display());
        let line =
            std::fs::read_to_string(&public).with_context(|| format!("cannot read {public}"));
        Ok(line?.trim().to_string())
    }

    /// Proves that the key alone opens the host.
    async fn login(&self) -> Reply<()> {
        let login = self
            .backend
            .net
            .key_login(self.target.clone(), self.key.clone());
        login.await
    }

    /// The same once sshd was told to read its config again: for a moment
    /// nobody is at the door, which is not the key being refused.
    async fn login_again(&self) -> Reply<()> {
        let mut knocks = 0;
        loop {
            match self.login().await {
                Err(shut) if shut.class != Class::Auth && knocks < 10 => knocks += 1,
                answer => return answer,
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// Names the key in the host's block, so that it is logged in to with it.
    fn name_key(&self) -> Result<()> {
        let mut sshc = self.backend.config_of(self.host)?;
        let key = self.key.to_string_lossy();
        if sshc.own_all(self.host, "IdentityFile").contains(&&*key) {
            return Ok(());
        }
        sshc.add(self.host, "IdentityFile", &key);
        self.backend.save(&sshc)
    }

    async fn run(&self) -> Reply<()> {
        let public = self.step("generate", async { Ok(self.public()?) }).await?;
        // One connection for everything that is changed on the host: once
        // passwords are off there may be no way to make another, and the
        // backup must still get back.
        let backup = async {
            let (target, password) = (self.target.clone(), self.password.clone());
            let host = self.backend.net.connect(target, password).await?;
            let no_root = "cannot back up sshd_config (it takes root, or sudo without a password)";
            sh(&host, backup(), no_root).await?;
            Ok(host)
        };
        let host = self.step("backup", backup).await?;

        // From here on the host is changed: a failure puts the backup back.
        let rest = async {
            let install = sh(
                &host,
                install(&public),
                "cannot add the key to authorized_keys",
            );
            self.step("install", install).await?;
            let verify = async {
                self.login().await?;
                Ok(self.name_key()?)
            };
            self.step("verify", verify).await?;
            let disable = async {
                sh(&host, disable(), "cannot turn off password login").await?;
                // Whoever is locked out now must not stay so.
                self.login_again().await
            };
            self.step("disable", disable).await
        };
        let Err(mut why) = rest.await else {
            return Ok(());
        };
        let restore = sh(&host, restore(), "cannot restore sshd_config");
        if self.step("restore", restore).await.is_err() {
            let left = format!(
                "sshd_config could NOT be restored from {CONFIG}.vssh.bak: check it by hand"
            );
            why.chain.insert(0, left);
        }
        Err(why)
    }
}

/// Runs a script on the host under `sh`, whatever its login shell is. It is
/// over at the first command that fails; `failing` says what that means.
async fn sh(host: &Remote, script: String, failing: &'static str) -> Reply<()> {
    let script = format!("set -e; {script}; echo vssh-ok");
    let out = host(format!("sh -c '{}'", script.replace('\'', r"'\''"))).await?;
    if out.lines().last() == Some("vssh-ok") {
        return Ok(());
    }
    Err(anyhow!("{}", out.trim_end()).context(failing).into())
}

/// Gives the host a key of its own and turns off its password login.
/// Answers with the path of the private key. It takes root on the host,
/// or sudo that asks no password; the backup stays there as
/// `/etc/ssh/sshd_config.vssh.bak`.
#[tauri::command]
pub async fn ssh_key_setup<R: Runtime>(
    app: AppHandle<R>,
    host: String,
    backend: State<'_, Backend>,
) -> Reply<PathBuf> {
    let (target, password) = backend.login(&host)?;
    // Next to the ssh config, under a name that is one file name whatever
    // the alias is.
    let plain = |c: char| c.is_ascii_alphanumeric() || "-_.".contains(c);
    let name: String = host
        .chars()
        .map(|c| if plain(c) { c } else { '_' })
        .collect();
    let key = backend
        .ssh_config
        .with_file_name(format!("vssh_{name}_ed25519"));
    let setup = Setup {
        app: &app,
        backend: &backend,
        host: &host,
        target,
        password,
        key,
    };
    setup.run().await?;
    Ok(setup.key)
}
