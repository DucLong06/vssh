//! Port forwarding: local (`-L`), remote (`-R`) and dynamic SOCKS5 (`-D`).

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use russh::client::Handle;
use tokio::io::{copy_bidirectional, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;

use super::{connect, Client, Target};
use crate::store::ssh_config::Forward;

async fn closed(h: &Handle<Client>) {
    while !h.is_closed() {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Runs one forward until the connection drops; dropping the future stops it
/// and closes every tunnelled connection. `ready` fires once it is listening.
// shortcut: one SSH connection per forward, share one per host if the extra
// handshakes ever matter.
pub async fn run(
    t: &Target,
    password: Option<&str>,
    f: Forward,
    ready: impl FnOnce(),
) -> Result<()> {
    if f.kind == 'R' {
        let h = connect(t, password, Some((f.host.clone(), f.hport))).await?;
        h.tcpip_forward(f.bind.as_str(), f.port.into())
            .await
            .context("server refused the remote forward")?;
        ready();
        closed(&h).await;
        bail!("connection closed");
    }
    let h = Arc::new(connect(t, password, None).await?);
    let listener = TcpListener::bind((f.bind.as_str(), f.port))
        .await
        .with_context(|| format!("cannot listen on {}:{}", f.bind, f.port))?;
    ready();
    // Owns the per-connection tasks, so they die with this future.
    let mut conns = JoinSet::new();
    loop {
        let (sock, peer) = tokio::select! {
            accepted = listener.accept() => accepted?,
            _ = closed(&h) => bail!("connection closed"),
        };
        while conns.try_join_next().is_some() {}
        let (h, f) = (h.clone(), f.clone());
        conns.spawn(async move {
            let _ = tunnel(&h, &f, sock, peer).await;
        });
    }
}

async fn tunnel(
    h: &Handle<Client>,
    f: &Forward,
    mut sock: TcpStream,
    peer: SocketAddr,
) -> Result<()> {
    let (host, port) = match f.kind {
        'D' => socks5_request(&mut sock).await?,
        _ => (f.host.clone(), f.hport),
    };
    let channel = h
        .channel_open_direct_tcpip(host, port.into(), peer.ip().to_string(), peer.port().into())
        .await;
    if f.kind == 'D' {
        // Reply: succeeded (0) or general failure (1), bound address unused.
        let status = u8::from(channel.is_err());
        sock.write_all(&[5, status, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
    }
    copy_bidirectional(&mut channel?.into_stream(), &mut sock).await?;
    Ok(())
}

/// Server side of the SOCKS5 handshake (RFC 1928): no authentication,
/// CONNECT only. Returns the requested destination.
async fn socks5_request<S: AsyncRead + AsyncWrite + Unpin>(s: &mut S) -> Result<(String, u16)> {
    let mut head = [0u8; 2];
    s.read_exact(&mut head).await?;
    ensure!(head[0] == 5, "not a SOCKS5 client");
    let mut methods = vec![0u8; head[1].into()];
    s.read_exact(&mut methods).await?;
    if !methods.contains(&0) {
        s.write_all(&[5, 0xff]).await?;
        bail!("SOCKS client requires authentication");
    }
    s.write_all(&[5, 0]).await?;

    let mut req = [0u8; 4];
    s.read_exact(&mut req).await?;
    let host = match (req[1], req[3]) {
        (1, 1) => {
            let mut ip = [0u8; 4];
            s.read_exact(&mut ip).await?;
            Ipv4Addr::from(ip).to_string()
        }
        (1, 3) => {
            let mut name = vec![0u8; s.read_u8().await?.into()];
            s.read_exact(&mut name).await?;
            String::from_utf8(name)?
        }
        (1, 4) => {
            let mut ip = [0u8; 16];
            s.read_exact(&mut ip).await?;
            Ipv6Addr::from(ip).to_string()
        }
        (cmd, atyp) => {
            // Command not supported (7) / address type not supported (8).
            let code = if cmd == 1 { 8 } else { 7 };
            s.write_all(&[5, code, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            bail!("unsupported SOCKS request (command {cmd}, address type {atyp})");
        }
    };
    Ok((host, s.read_u16().await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn handshake(request: &[u8]) -> (Result<(String, u16)>, Vec<u8>) {
        let (mut client, mut server) = tokio::io::duplex(256);
        client.write_all(request).await.unwrap();
        // Half-close, so a truncated request ends in EOF instead of a hang.
        client.shutdown().await.unwrap();
        let res = socks5_request(&mut server).await;
        drop(server);
        let mut reply = Vec::new();
        client.read_to_end(&mut reply).await.unwrap();
        (res, reply)
    }

    #[tokio::test]
    async fn socks5_connect_requests() {
        let (res, reply) = handshake(b"\x05\x01\x00\x05\x01\x00\x03\x0bexample.com\x01\xbb").await;
        assert_eq!(res.unwrap(), ("example.com".to_string(), 443));
        assert_eq!(reply, [5, 0]);

        let (res, _) = handshake(b"\x05\x02\x02\x00\x05\x01\x00\x01\x0a\x00\x00\x07\x00\x50").await;
        assert_eq!(res.unwrap(), ("10.0.0.7".to_string(), 80));

        let mut v6 = b"\x05\x01\x00\x05\x01\x00\x04".to_vec();
        v6.extend_from_slice(&Ipv6Addr::LOCALHOST.octets());
        v6.extend_from_slice(&[0x1f, 0x90]);
        assert_eq!(handshake(&v6).await.0.unwrap(), ("::1".to_string(), 8080));
    }

    #[tokio::test]
    async fn socks5_rejects_what_it_cannot_do() {
        // SOCKS4, auth-only client, BIND command, truncated request.
        assert!(handshake(b"\x04\x01\x00\x50").await.0.is_err());
        let (res, reply) = handshake(b"\x05\x01\x02").await;
        assert!(res.is_err());
        assert_eq!(reply, [5, 0xff]);
        let (res, reply) = handshake(b"\x05\x01\x00\x05\x02\x00\x01\x7f\x00\x00\x01\x00\x50").await;
        assert!(res.is_err());
        assert_eq!(reply[..4], [5, 0, 5, 7]);
        assert!(handshake(b"\x05\x01\x00\x05\x01\x00\x03\x0bexam")
            .await
            .0
            .is_err());
    }
}
