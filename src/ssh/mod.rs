//! SSH transport on russh: connect, host key check, auth, shell and exec.

pub mod forward;

use std::borrow::Cow;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use russh::client::{self, Handle};
use russh::keys::agent::client::AgentClient;
use russh::keys::known_hosts::{known_host_keys_path, learn_known_hosts_path};
use russh::keys::{
    load_public_key, load_secret_key, Algorithm, HashAlg, PrivateKeyWithHashAlg, PublicKey,
    PublicKeyOrCertificate,
};
use russh::{Channel, ChannelMsg};
use tokio::io::{copy_bidirectional, AsyncRead, AsyncWrite};
use tokio::net::{lookup_host, TcpStream};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::conn_err::Fail;
use crate::store::ssh_config::SshConfig;

#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// Private key files to try, in order.
    pub keys: Vec<PathBuf>,
    /// `RemoteCommand` of the ssh config: run instead of the login shell.
    pub remote_command: Option<String>,
}

impl Target {
    /// Resolves an alias the way `ssh alias` would.
    pub fn from_config(c: &SshConfig, alias: &str) -> Result<Self> {
        for key in ["ProxyJump", "ProxyCommand"] {
            if c.get(alias, key)
                .is_some_and(|v| !v.eq_ignore_ascii_case("none"))
            {
                bail!("{key} is not supported yet");
            }
        }
        let host = c.get(alias, "HostName").unwrap_or(alias);
        crate::proto::ensure_ssh(host)?;
        let home = crate::config::home();
        let mut keys: Vec<PathBuf> = c
            .get_all(alias, "IdentityFile")
            .iter()
            .map(|p| match p.strip_prefix("~/") {
                Some(rest) => home.join(rest),
                None => PathBuf::from(p),
            })
            .collect();
        if keys.is_empty() {
            keys = default_keys();
        }
        Ok(Self {
            host: host.to_string(),
            port: c
                .get(alias, "Port")
                .and_then(|p| p.parse().ok())
                .unwrap_or(22),
            user: c
                .get(alias, "User")
                .map_or_else(current_user, str::to_string),
            keys,
            remote_command: c
                .get(alias, "RemoteCommand")
                .filter(|cmd| !cmd.eq_ignore_ascii_case("none"))
                .map(str::to_string),
        })
    }

    /// Adds this target to the ssh config as a host and returns the alias it
    /// got: the host name, or `user@host` if that name is already a host.
    /// Only `HostName`, `User` and a non-default `Port` are written; a
    /// password never goes into this file.
    pub fn save_to(&self, c: &mut SshConfig) -> Result<String> {
        if self.host.contains(['*', '?', '!', '#', '"', '=']) {
            bail!("{} cannot be used as a host alias", self.host);
        }
        let taken = c.hosts();
        let with_user = format!("{}@{}", self.user, self.host);
        let alias = [self.host.clone(), with_user]
            .into_iter()
            .find(|a| !taken.contains(a))
            .with_context(|| format!("{}@{} is already a saved host", self.user, self.host))?;
        c.add_host(&alias);
        c.set(&alias, "HostName", &self.host);
        c.set(&alias, "User", &self.user);
        if self.port != 22 {
            c.set(&alias, "Port", &self.port.to_string());
        }
        Ok(alias)
    }

    /// A one-off target typed as `[user@]host[:port]`; IPv6 as `[::1]:22`.
    /// Nothing is read from or written to the ssh config.
    pub fn parse(spec: &str) -> Result<Self> {
        let (user, rest) = match spec.trim().rsplit_once('@') {
            Some((user, rest)) => (user.to_string(), rest),
            None => (current_user(), spec.trim()),
        };
        let (host, port) = match rest.rsplit_once(':') {
            // More than one colon without brackets is a bare IPv6 address.
            Some((host, _)) if host.contains(':') && !host.starts_with('[') => (rest, "22"),
            Some((host, port)) => (host, port),
            None => (rest, "22"),
        };
        let host = host.trim_start_matches('[').trim_end_matches(']');
        let bad = |s: &str| s.is_empty() || s.contains(|c: char| c.is_whitespace() || c == '@');
        if bad(&user) || bad(host) {
            bail!("expected user@host or user@host:port");
        }
        crate::proto::ensure_ssh(host)?;
        Ok(Self {
            host: host.to_string(),
            port: port.parse().context("port must be a number up to 65535")?,
            user,
            keys: default_keys(),
            remote_command: None,
        })
    }
}

fn current_user() -> String {
    std::env::var("USER").unwrap_or_else(|_| "root".into())
}

fn default_keys() -> Vec<PathBuf> {
    let dir = crate::config::home().join(".ssh");
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .map(|k| dir.join(k))
        .into()
}

pub struct Client {
    host: String,
    port: u16,
    /// Where connections arriving over a remote (`-R`) forward are sent.
    remote_fwd: Option<(String, u16)>,
}

impl client::Handler for Client {
    type Error = anyhow::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            bail!("host certificates are not supported");
        };
        verify_host_key(&self.host, self.port, key, &known_hosts_path())?;
        Ok(true)
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<client::Msg>,
        _connected_address: &str,
        _connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<()> {
        // Dropping `reply` rejects: no forward requested, or its target is down.
        let Some(to) = self.remote_fwd.clone() else {
            return Ok(());
        };
        if let Ok(mut sock) = TcpStream::connect(to).await {
            reply.accept().await;
            tokio::spawn(async move {
                let _ = copy_bidirectional(&mut channel.into_stream(), &mut sock).await;
            });
        }
        Ok(())
    }
}

fn known_hosts_path() -> PathBuf {
    crate::config::home().join(".ssh/known_hosts")
}

/// `known_hosts` policy, as `StrictHostKeyChecking accept-new`: a host with
/// no recorded key is recorded on first contact; a host with recorded keys
/// must present one of them. A key of another type than the recorded one is
/// refused too, otherwise an impostor could get in just by switching type.
pub fn verify_host_key(host: &str, port: u16, key: &PublicKey, known_hosts: &Path) -> Result<()> {
    let known = known_host_keys_path(host, port, known_hosts)
        .with_context(|| format!("cannot read {}", known_hosts.display()))?;
    if known.iter().any(|(_, k)| k.key_data() == key.key_data()) {
        return Ok(());
    }
    if let Some((line, _)) = known.first() {
        return Err(Fail::HostKey {
            host: host.to_string(),
            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
            line: *line,
            file: known_hosts.to_path_buf(),
        }
        .into());
    }
    learn_known_hosts_path(host, port, key, known_hosts)
        .with_context(|| format!("cannot write {}", known_hosts.display()))
}

/// True if `known_hosts` already has a key for this target: it has been
/// connected to before, so talking to it again trusts nothing new.
pub fn is_known(t: &Target) -> bool {
    known_host_keys_path(&t.host, t.port, known_hosts_path()).is_ok_and(|known| !known.is_empty())
}

/// Host key algorithms to offer, the types already on record first (as
/// OpenSSH does), so a known host proves the key we know.
fn host_key_algorithms(known: &[PublicKey]) -> Vec<Algorithm> {
    let recorded = known.iter().flat_map(|k| match k.algorithm() {
        // A recorded ssh-rsa key is also valid for the SHA-2 signature variants.
        Algorithm::Rsa { .. } => [Some(HashAlg::Sha512), Some(HashAlg::Sha256), None]
            .map(|hash| Algorithm::Rsa { hash })
            .to_vec(),
        other => vec![other],
    });
    let defaults = client::Config::default().preferred.key;
    let mut all: Vec<Algorithm> = vec![];
    for algorithm in recorded.chain(defaults.iter().cloned()) {
        if !all.contains(&algorithm) {
            all.push(algorithm);
        }
    }
    all
}

/// How long a TCP connect, and then the SSH handshake, may take.
const PATIENCE: Duration = Duration::from_secs(10);

/// Resolves the name and opens the TCP connection. The error says which of
/// the two failed, and at which address.
async fn dial(host: &str, port: u16) -> Result<TcpStream> {
    let dns = || Fail::Dns {
        host: host.to_string(),
    };
    let addrs = lookup_host((host, port))
        .await
        .map_err(|e| anyhow::Error::new(e).context(dns()))?;
    // Every address is tried; the last failure is the one reported.
    let mut last = anyhow::Error::new(dns());
    for addr in addrs {
        let at = addr.to_string();
        last = match tokio::time::timeout(PATIENCE, TcpStream::connect(addr)).await {
            Ok(Ok(sock)) => return Ok(sock),
            Ok(Err(e)) if e.kind() == ErrorKind::ConnectionRefused => {
                anyhow::Error::new(e).context(Fail::Refused { addr: at })
            }
            Ok(Err(e)) => anyhow::Error::new(e).context(Fail::Unreachable { addr: at }),
            Err(_) => Fail::Timeout {
                addr: at,
                secs: PATIENCE.as_secs(),
            }
            .into(),
        };
    }
    Err(last)
}

/// The SSH handshake over an open stream, host key check included.
async fn handshake<S>(
    t: &Target,
    stream: S,
    remote_fwd: Option<(String, u16)>,
) -> Result<Handle<Client>>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let mut config = client::Config {
        keepalive_interval: Some(Duration::from_secs(30)),
        ..Default::default()
    };
    let known = known_host_keys_path(&t.host, t.port, known_hosts_path()).unwrap_or_default();
    let known: Vec<PublicKey> = known.into_iter().map(|(_, key)| key).collect();
    config.preferred.key = Cow::Owned(host_key_algorithms(&known));
    let handler = Client {
        host: t.host.clone(),
        port: t.port,
        remote_fwd,
    };
    let shaking = client::connect_stream(Arc::new(config), stream, handler);
    let slow = |_| Fail::Timeout {
        addr: format!("{}:{}", t.host, t.port),
        secs: PATIENCE.as_secs(),
    };
    tokio::time::timeout(PATIENCE, shaking)
        .await
        .map_err(slow)?
}

/// Authenticates: agent, then key files, then the password. If all of it is
/// refused the error says what was tried (`Fail::Auth`).
async fn login(h: &mut Handle<Client>, t: &Target, password: Option<&str>) -> Result<()> {
    let rsa_hash = h.best_supported_rsa_hash().await?.flatten();
    let mut keys = 0;

    if let Ok(mut agent) = AgentClient::connect_env().await {
        let mut ids = agent.request_identities().await.unwrap_or_default();
        // Keys named by IdentityFile go first, so an agent full of unrelated
        // keys does not use up the server's MaxAuthTries.
        let wanted: Vec<_> = t
            .keys
            .iter()
            .filter_map(|k| load_public_key(format!("{}.pub", k.display())).ok())
            .collect();
        ids.sort_by_key(|id| {
            !wanted
                .iter()
                .any(|w| w.key_data() == id.public_key().key_data())
        });
        for id in ids {
            let key = id.public_key().into_owned();
            keys += 1;
            let res = h
                .authenticate_publickey_with(&t.user, key, rsa_hash, &mut agent)
                .await;
            if res.is_ok_and(|r| r.success()) {
                return Ok(());
            }
        }
    }
    for path in &t.keys {
        // Missing and passphrase-protected files are skipped (use the agent).
        let Ok(key) = load_secret_key(path, None) else {
            continue;
        };
        let key = PrivateKeyWithHashAlg::new(Arc::new(key), rsa_hash);
        keys += 1;
        if h.authenticate_publickey(&t.user, key).await?.success() {
            return Ok(());
        }
    }
    if let Some(pw) = password {
        if h.authenticate_password(&t.user, pw).await?.success() {
            return Ok(());
        }
    }
    Err(Fail::Auth {
        user: t.user.clone(),
        host: t.host.clone(),
        keys,
        password: password.is_some(),
    }
    .into())
}

/// Connects and authenticates. A failure carries a `Fail` saying which step
/// it was: name lookup, TCP, host key, or login.
pub async fn connect(
    t: &Target,
    password: Option<&str>,
    remote_fwd: Option<(String, u16)>,
) -> Result<Handle<Client>> {
    let sock = dial(&t.host, t.port).await?;
    let _ = sock.set_nodelay(true);
    let mut h = handshake(t, sock, remote_fwd).await?;
    login(&mut h, t, password).await?;
    Ok(h)
}

pub enum SessionCmd {
    Data(Vec<u8>),
    /// Columns, rows.
    Resize(u16, u16),
}

/// What a session runs when it starts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Login {
    /// Run this instead of the login shell (`RemoteCommand`).
    pub command: Option<String>,
    /// Type this into the session once it is up (a login script).
    pub script: Option<String>,
}

impl Login {
    /// The bytes that "type" the script: each line followed by Enter, as if
    /// it came from the keyboard. Blank lines are dropped.
    pub fn script_input(&self) -> Vec<u8> {
        let lines = self.script.iter().flat_map(|s| s.lines());
        let typed = lines.filter(|l| !l.trim().is_empty());
        typed.flat_map(|l| [l.as_bytes(), b"\r"].concat()).collect()
    }
}

/// Runs an interactive session on a PTY until either side closes it. The
/// connection is only borrowed, so other channels can run next to the shell.
pub async fn shell(
    h: &Handle<Client>,
    (cols, rows): (u16, u16),
    login: Login,
    mut rx: UnboundedReceiver<SessionCmd>,
    on_data: impl Fn(&[u8]),
) -> Result<()> {
    let mut ch = h.channel_open_session().await?;
    ch.request_pty(false, "xterm-256color", cols.into(), rows.into(), 0, 0, &[])
        .await?;
    match &login.command {
        Some(command) => ch.exec(false, command.as_str()).await?,
        None => ch.request_shell(false).await?,
    }
    let script = login.script_input();
    if !script.is_empty() {
        // The PTY holds it until the shell starts reading.
        ch.data(&script[..]).await?;
    }
    loop {
        tokio::select! {
            msg = ch.wait() => match msg {
                Some(ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. }) => {
                    on_data(&data)
                }
                Some(ChannelMsg::Close) | None => return Ok(()),
                _ => {}
            },
            cmd = rx.recv() => match cmd {
                Some(SessionCmd::Data(d)) => ch.data(&d[..]).await?,
                Some(SessionCmd::Resize(c, r)) => ch.window_change(c.into(), r.into(), 0, 0).await?,
                None => return Ok(()),
            },
        }
    }
}

/// Runs one command and returns its combined stdout and stderr; a non-zero
/// exit status is appended.
pub async fn exec(h: &Handle<Client>, command: &str) -> Result<String> {
    let mut ch = h.channel_open_session().await?;
    ch.exec(true, command).await?;
    let mut out = Vec::new();
    let mut status = 0;
    while let Some(msg) = ch.wait().await {
        match msg {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                out.extend_from_slice(&data)
            }
            ChannelMsg::ExitStatus { exit_status } => status = exit_status,
            _ => {}
        }
    }
    let mut out = String::from_utf8_lossy(&out).into_owned();
    if status != 0 {
        out.push_str(&format!("[exit status {status}]\n"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::ssh_key::public::{Ed25519PublicKey, KeyData};
    use std::fs;

    fn key(seed: u8) -> PublicKey {
        KeyData::Ed25519(Ed25519PublicKey([seed; 32])).into()
    }

    #[test]
    fn host_key_policy() {
        let dir = std::env::temp_dir().join(format!("vssh-hostkey-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("known_hosts");

        // Unknown host, no file yet: accepted and recorded.
        verify_host_key("alpha", 22, &key(1), &path).unwrap();
        let learned = fs::read_to_string(&path).unwrap();
        assert!(learned.contains("alpha ssh-ed25519 "));

        // Known host, same key: accepted, nothing written.
        verify_host_key("alpha", 22, &key(1), &path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), learned);

        // Known host, different key: refused, nothing written.
        let err = verify_host_key("alpha", 22, &key(2), &path).unwrap_err();
        assert!(err.to_string().starts_with("HOST KEY MISMATCH for alpha"));
        assert_eq!(fs::read_to_string(&path).unwrap(), learned);

        // Another port or name is another host with its own record.
        verify_host_key("alpha", 2222, &key(2), &path).unwrap();
        verify_host_key("beta", 22, &key(3), &path).unwrap();
        assert!(verify_host_key("alpha", 2222, &key(1), &path).is_err());
        assert!(verify_host_key("beta", 22, &key(3), &path).is_ok());
        assert!(fs::read_to_string(&path).unwrap().contains("[alpha]:2222 "));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn key_of_another_type_does_not_count_as_unknown() {
        let dir = std::env::temp_dir().join(format!("vssh-hostkey-type-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("known_hosts");
        // The host is on record with an ECDSA key; an Ed25519 key shows up.
        let ecdsa = "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBKJHpxtF7tWbtXLUK2aKqXLokliWZyiyV1vJ+nX2Lpp2cR0JzQZRJ40tKdv695CMBBwbVQPIlCNsa1qBi3wxGR8=";
        fs::write(&path, format!("gamma {ecdsa}\n")).unwrap();
        let before = fs::read_to_string(&path).unwrap();

        let err = verify_host_key("gamma", 22, &key(1), &path).unwrap_err();
        assert!(err.to_string().contains("line 1"), "{err}");
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn known_key_types_are_offered_first() {
        let default = client::Config::default().preferred.key.to_vec();
        assert_eq!(host_key_algorithms(&[]), default);

        let rsa = "AAAAB3NzaC1yc2EAAAADAQABAAABAQDugpu62IWX1u5o2utZw5MnT2qlmiLrNpeS/P7k82bjg4y3F/9vF4Skbj6wp8S5IDUkQki/Iu17UTKVlJ9AsxR9skpCLAu2UOC63Np1HA0x+gQ+ZEZFTEaeUc/ljIwVmy5SATY6FRwJXIwYlARdeXkcrQ6mNtUErW0ldoavPhnbttoBuiyaHB/brJ1z/JU+3KZYYaP8H0qgFkvB6bdkMNud2f5w21camhfx404Im3QUN+xkgSWqYwbVKd/zaPJiwsxfrp4hQbHvkXrT4hEOOoOLEQahCrXjDqtQMYuDswrTc1ENlVOI7NkndSMpMV0wZ+/5U6GaorVtt54zzOUKEVKf";
        let rsa = russh::keys::parse_public_key_base64(rsa).unwrap();
        let offered = host_key_algorithms(&[rsa]);
        assert!(matches!(
            offered[0],
            Algorithm::Rsa {
                hash: Some(HashAlg::Sha512)
            }
        ));
        assert!(matches!(offered[2], Algorithm::Rsa { hash: None }));
        // Everything else is still offered, once.
        assert_eq!(offered.len(), default.len());
    }

    #[test]
    fn login_script_is_typed_line_by_line() {
        let login = |script: &str| Login {
            command: None,
            script: Some(script.to_string()),
        };
        assert_eq!(login("uptime").script_input(), b"uptime\r");
        assert_eq!(
            login("cd /srv/app\n\n  \ngit status\n").script_input(),
            b"cd /srv/app\rgit status\r"
        );
        assert!(login("").script_input().is_empty());
        assert!(Login::default().script_input().is_empty());
    }

    #[test]
    fn remote_command_comes_from_the_ssh_config() {
        let c = SshConfig::parse(
            "Host logs\n  RemoteCommand journalctl -f -u app\n\
             Host plain\n  RemoteCommand none\n\
             Host inherits\n\
             Host *\n  User admin\n",
        );
        let command = |alias| Target::from_config(&c, alias).unwrap().remote_command;
        assert_eq!(command("logs").as_deref(), Some("journalctl -f -u app"));
        assert_eq!(command("plain"), None);
        assert_eq!(command("inherits"), None);
        assert_eq!(Target::parse("a@b").unwrap().remote_command, None);
    }

    #[test]
    fn saving_a_quick_connect_target() {
        let mut c = SshConfig::parse(
            "Host old\n    HostName 10.0.0.9\n\nHost *\n    User fallback\n    Port 2200\n",
        );
        let target = Target::parse("deploy@web.example:2222").unwrap();

        // First save: the alias is the host name, and the block resolves back to the target.
        assert_eq!(target.save_to(&mut c).unwrap(), "web.example");
        let back = Target::from_config(&c, "web.example").unwrap();
        assert_eq!(
            (back.host, back.user, back.port),
            ("web.example".into(), "deploy".into(), 2222)
        );
        // It sits ahead of `Host *`, so the wildcard defaults do not override it.
        let text = c.to_string();
        assert!(text.contains(
            "Host web.example\n    HostName web.example\n    User deploy\n    Port 2222\n\nHost *\n"
        ));

        // Same host again: `user@host`; a third time there is no free name left.
        assert_eq!(target.save_to(&mut c).unwrap(), "deploy@web.example");
        let err = target.save_to(&mut c).unwrap_err().to_string();
        assert_eq!(err, "deploy@web.example is already a saved host");
        // Another user on the same host still gets a name.
        let other = Target::parse("root@web.example").unwrap();
        assert_eq!(other.save_to(&mut c).unwrap(), "root@web.example");
        // The default port is left out.
        assert_eq!(c.own("root@web.example", "Port"), "");
        assert_eq!(
            c.hosts(),
            [
                "old",
                "web.example",
                "deploy@web.example",
                "root@web.example"
            ]
        );

        // A name that would be a pattern, not a host, is refused before anything is written.
        let before = c.to_string();
        assert!(Target::parse("root@web*").unwrap().save_to(&mut c).is_err());
        assert_eq!(c.to_string(), before);
    }

    #[test]
    fn quick_connect_targets() {
        let t = |spec| Target::parse(spec).unwrap();
        let parts = |t: &Target| (t.user.clone(), t.host.clone(), t.port);
        assert_eq!(
            parts(&t("root@10.0.0.5")),
            ("root".into(), "10.0.0.5".into(), 22)
        );
        assert_eq!(
            parts(&t(" deploy@web.example:2222 ")),
            ("deploy".into(), "web.example".into(), 2222)
        );
        assert_eq!(
            parts(&t("me@[::1]:2200")),
            ("me".into(), "::1".into(), 2200)
        );
        assert_eq!(parts(&t("me@fe80::1")), ("me".into(), "fe80::1".into(), 22));
        assert_eq!(t("box").host, "box");
        assert_eq!(t("box").user, current_user());
        assert_eq!(t("a@b").keys, default_keys());
        for bad in [
            "",
            "@host",
            "user@",
            "user@ho st",
            "a@b:port",
            "a@b:70000",
            "a@telnet://b",
        ] {
            assert!(Target::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn target_resolution() {
        let c = SshConfig::parse(
            "Host web\n  HostName 10.0.0.1\n  Port 2222\n  IdentityFile /keys/web\n\
             Host jump\n  ProxyJump bastion\n\
             Host direct\n  ProxyJump none\n\
             Host tty\n  HostName serial:///dev/ttyUSB0\n\
             Host *\n  User admin\n",
        );
        let t = Target::from_config(&c, "web").unwrap();
        assert_eq!(
            (t.host.as_str(), t.port, t.user.as_str()),
            ("10.0.0.1", 2222, "admin")
        );
        assert_eq!(t.keys, [PathBuf::from("/keys/web")]);

        // No HostName: the alias is the address; no IdentityFile: the defaults.
        let t = Target::from_config(&c, "direct").unwrap();
        assert_eq!((t.host.as_str(), t.port), ("direct", 22));
        assert_eq!(t.keys.len(), 3);

        let err = |alias| Target::from_config(&c, alias).unwrap_err().to_string();
        assert_eq!(err("jump"), "ProxyJump is not supported yet");
        assert_eq!(err("tty"), "Serial is not implemented yet");
    }
}
