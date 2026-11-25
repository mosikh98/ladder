//! Loopback TCP relay. Each accepted connection is opened onward through a
//! SOCKS5 proxy (Cloudflare WARP or a Worker) to one fixed host:port.
//! OpenVPN, Stealth and WStunnel then dial 127.0.0.1 and still speak their
//! own protocol to the real server.

use anyhow::{anyhow, Result};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone, Debug)]
pub enum Onward {
    /// Connect TCP directly. Used by tests and by a line that can reach the host.
    Direct,
    /// RFC 1928 CONNECT, no authentication.
    Socks5(SocketAddr),
}

pub async fn socks5_connect(proxy: SocketAddr, host: &str, port: u16) -> Result<TcpStream> {
    let mut s = TcpStream::connect(proxy).await?;
    s.write_all(&[0x05, 0x01, 0x00]).await?;
    let mut greet = [0u8; 2];
    s.read_exact(&mut greet).await?;
    if greet != [0x05, 0x00] {
        return Err(anyhow!("socks5 proxy refused no-auth ({greet:?})"));
    }
    let hb = host.as_bytes();
    if hb.len() > 255 {
        return Err(anyhow!("socks5 host too long"));
    }
    let mut req = vec![0x05, 0x01, 0x00, 0x03, hb.len() as u8];
    req.extend_from_slice(hb);
    req.push((port >> 8) as u8);
    req.push((port & 0xff) as u8);
    s.write_all(&req).await?;
    let mut head = [0u8; 4];
    s.read_exact(&mut head).await?;
    if head[1] != 0x00 {
        return Err(anyhow!("socks5 connect failed, reply {}", head[1]));
    }
    let skip = match head[3] {
        0x01 => 4,
        0x04 => 16,
        0x03 => {
            let mut l = [0u8; 1];
            s.read_exact(&mut l).await?;
            l[0] as usize
        }
        other => return Err(anyhow!("socks5 bad atyp {other}")),
    };
    let mut junk = vec![0u8; skip + 2];
    s.read_exact(&mut junk).await?;
    Ok(s)
}

async fn dial(onward: &Onward, host: &str, port: u16) -> Result<TcpStream> {
    match onward {
        Onward::Direct => {
            let addr = tokio::net::lookup_host((host, port))
                .await?
                .next()
                .ok_or_else(|| anyhow!("no address for {host}"))?;
            Ok(TcpStream::connect(addr).await?)
        }
        Onward::Socks5(proxy) => socks5_connect(*proxy, host, port).await,
    }
}

async fn pump(mut a: TcpStream, mut b: TcpStream) {
    let (mut ar, mut aw) = a.split();
    let (mut br, mut bw) = b.split();
    let _ = tokio::join!(
        tokio::io::copy(&mut ar, &mut bw),
        tokio::io::copy(&mut br, &mut aw),
    );
}

/// Serve until the task is dropped. `listen` may use port 0.
pub async fn serve(listen: SocketAddr, onward: Onward, host: String, port: u16) -> Result<SocketAddr> {
    let listener = TcpListener::bind(listen).await?;
    let local = listener.local_addr()?;
    tokio::spawn(async move {
        loop {
            let Ok((client, _)) = listener.accept().await else {
                break;
            };
            let onward = onward.clone();
            let host = host.clone();
            tokio::spawn(async move {
                if let Ok(remote) = dial(&onward, &host, port).await {
                    pump(client, remote).await;
                }
            });
        }
    });
    Ok(local)
}

/// Encode a SOCKS5 CONNECT request. Tested without a network.
pub fn encode_connect(host: &str, port: u16) -> Vec<u8> {
    let hb = host.as_bytes();
    let mut req = vec![0x05, 0x01, 0x00, 0x03, hb.len() as u8];
    req.extend_from_slice(hb);
    req.push((port >> 8) as u8);
    req.push((port & 0xff) as u8);
    req
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn connect_request_is_domain_form() {
        let r = encode_connect("edge.example", 443);
        assert_eq!(&r[..5], &[0x05, 0x01, 0x00, 0x03, 12]);
        assert_eq!(&r[r.len() - 2..], &[0x01, 0xbb]);
    }

    #[tokio::test]
    async fn direct_relay_copies_bytes() {
        let echo = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_addr = echo.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = echo.accept().await.unwrap();
            let mut buf = [0u8; 16];
            let n = s.read(&mut buf).await.unwrap();
            s.write_all(&buf[..n]).await.unwrap();
        });
        let local = serve(
            "127.0.0.1:0".parse().unwrap(),
            Onward::Direct,
            echo_addr.ip().to_string(),
            echo_addr.port(),
        )
        .await
        .unwrap();
        let mut c = TcpStream::connect(local).await.unwrap();
        c.write_all(b"ping").await.unwrap();
        let mut buf = [0u8; 4];
        c.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"ping");
    }

    #[tokio::test]
    async fn socks5_handshake_against_local_proxy() {
        let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy.local_addr().unwrap();
        let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_addr = target.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = target.accept().await.unwrap();
            let mut buf = [0u8; 8];
            let n = s.read(&mut buf).await.unwrap();
            s.write_all(&buf[..n]).await.unwrap();
        });
        tokio::spawn(async move {
            let (mut c, _) = proxy.accept().await.unwrap();
            let mut g = [0u8; 3];
            c.read_exact(&mut g).await.unwrap();
            c.write_all(&[0x05, 0x00]).await.unwrap();
            let mut head = [0u8; 5];
            c.read_exact(&mut head).await.unwrap();
            let n = head[4] as usize;
            let mut rest = vec![0u8; n + 2];
            c.read_exact(&mut rest).await.unwrap();
            // reply: succeeded, IPv4 0.0.0.0:0
            c.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await
                .unwrap();
            let remote = TcpStream::connect(target_addr).await.unwrap();
            let (mut cr, mut cw) = c.split();
            let (mut rr, mut rw) = remote.into_split();
            let _ = tokio::join!(
                tokio::io::copy(&mut cr, &mut rw),
                tokio::io::copy(&mut rr, &mut cw),
            );
        });
        let mut s = socks5_connect(proxy_addr, "ignored.example", 9).await.unwrap();
        s.write_all(b"abc").await.unwrap();
        let mut buf = [0u8; 3];
        s.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"abc");
    }
}
