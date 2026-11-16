//! SSH transport on russh: connect, host key check, auth, shell and exec.

pub mod forward;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::{fmt, io};

use anyhow::{anyhow, bail, Result};
use russh::client::{self, Handle};
use russh::keys::agent::client::AgentClient;
use russh::keys::known_hosts::learn_known_hosts;
use russh::keys::{
    check_known_hosts, load_public_key, load_secret_key, PrivateKeyWithHashAlg,
    PublicKeyOrCertificate,
};
use russh::{Channel, ChannelMsg};
use tokio::io::copy_bidirectional;
use tokio::net::TcpStream;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::store::ssh_config::SshConfig;

/// Every offered method was refused; the caller may retry with a password.
#[derive(Debug)]
pub struct AuthFailed;

impl fmt::Display for AuthFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("authentication failed")
    }
}

impl std::error::Error for AuthFailed {}

#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// Private key files to try, in order.
    pub keys: Vec<PathBuf>,
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
            keys = ["id_ed25519", "id_ecdsa", "id_rsa"]
                .map(|k| home.join(".ssh").join(k))
                .into();
        }
        Ok(Self {
            host: host.to_string(),
            port: c
                .get(alias, "Port")
                .and_then(|p| p.parse().ok())
                .unwrap_or(22),
            user: c
                .get(alias, "User")
                .map(str::to_string)
                .or_else(|| std::env::var("USER").ok())
                .unwrap_or_else(|| "root".into()),
            keys,
        })
    }
}

pub struct Client {
    host: String,
    port: u16,
    /// Where connections arriving over a remote (`-R`) forward are sent.
    remote_fwd: Option<(String, u16)>,
}

impl client::Handler for Client {
    type Error = anyhow::Error;

    /// `known_hosts` policy of `StrictHostKeyChecking accept-new`: an unknown
    /// host is recorded on first contact, a changed key is always refused.
    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            bail!("host certificates are not supported");
        };
        match check_known_hosts(&self.host, self.port, key) {
            Ok(true) => Ok(true),
            Ok(false) => {
                learn_known_hosts(&self.host, self.port, key)?;
                Ok(true)
            }
            // A missing known_hosts file just means the host is not known yet.
            Err(russh::keys::Error::IO(e)) if e.kind() == io::ErrorKind::NotFound => {
                learn_known_hosts(&self.host, self.port, key)?;
                Ok(true)
            }
            Err(e) => bail!(
                "HOST KEY MISMATCH for {}: {e}. Refusing to connect; \
                 fix ~/.ssh/known_hosts if the key really changed",
                self.host
            ),
        }
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

/// Connects and authenticates: agent, then key files, then the password.
pub async fn connect(
    t: &Target,
    password: Option<&str>,
    remote_fwd: Option<(String, u16)>,
) -> Result<Handle<Client>> {
    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(30)),
        nodelay: true,
        ..Default::default()
    });
    let handler = Client {
        host: t.host.clone(),
        port: t.port,
        remote_fwd,
    };
    let connecting = client::connect(config, (t.host.as_str(), t.port), handler);
    let mut h = tokio::time::timeout(Duration::from_secs(10), connecting)
        .await
        .map_err(|_| anyhow!("connection to {}:{} timed out", t.host, t.port))??;
    let rsa_hash = h.best_supported_rsa_hash().await?.flatten();

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
            let res = h
                .authenticate_publickey_with(&t.user, key, rsa_hash, &mut agent)
                .await;
            if res.is_ok_and(|r| r.success()) {
                return Ok(h);
            }
        }
    }
    for path in &t.keys {
        // Missing and passphrase-protected files are skipped (use the agent).
        let Ok(key) = load_secret_key(path, None) else {
            continue;
        };
        let key = PrivateKeyWithHashAlg::new(Arc::new(key), rsa_hash);
        if h.authenticate_publickey(&t.user, key).await?.success() {
            return Ok(h);
        }
    }
    if let Some(pw) = password {
        if h.authenticate_password(&t.user, pw).await?.success() {
            return Ok(h);
        }
    }
    Err(AuthFailed.into())
}

pub enum SessionCmd {
    Data(Vec<u8>),
    /// Columns, rows.
    Resize(u16, u16),
}

/// Runs an interactive shell on a PTY until either side closes it.
pub async fn shell(
    h: Handle<Client>,
    (cols, rows): (u16, u16),
    mut rx: UnboundedReceiver<SessionCmd>,
    on_data: impl Fn(&[u8]),
) -> Result<()> {
    let mut ch = h.channel_open_session().await?;
    ch.request_pty(false, "xterm-256color", cols.into(), rows.into(), 0, 0, &[])
        .await?;
    ch.request_shell(false).await?;
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
