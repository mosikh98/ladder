//! Upstream proxies in front of Psiphon. A quick test only checks that the
//! proxy relays one TCP connection to an HTTPS port. Carrying a Psiphon tunnel
//! is a separate step: method X puts the URL in `UpstreamProxyURL` and the
//! official core has to actually come up inside the budget.

use crate::relay::{socks5_connect, Onward};
use anyhow::{anyhow, Result};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::timeout;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxyUrl {
    pub raw: String,
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxyKind {
    Http,
    Socks5,
}

pub fn parse_proxy(line: &str) -> Result<ProxyUrl> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return Err(anyhow!("empty"));
    }
    let (kind, rest) = if let Some(r) = line.strip_prefix("socks5://") {
        (ProxyKind::Socks5, r)
    } else if let Some(r) = line.strip_prefix("http://") {
        (ProxyKind::Http, r)
    } else {
        return Err(anyhow!("proxy must start with http:// or socks5://"));
    };
    let rest = rest.split('@').next_back().unwrap_or(rest);
    let (host, port_s) = rest
        .rsplit_once(':')
        .ok_or_else(|| anyhow!("proxy needs host:port"))?;
    let host = host.trim_matches(|c| c == '[' || c == ']').to_string();
    if host.is_empty() || host.contains('/') || host.contains(' ') {
        return Err(anyhow!("bad proxy host"));
    }
    let port: u16 = port_s.parse().map_err(|_| anyhow!("bad proxy port"))?;
    if port == 0 {
        return Err(anyhow!("bad proxy port"));
    }
    Ok(ProxyUrl {
        raw: line.to_string(),
        kind,
        host,
        port,
    })
}

pub fn parse_list(text: &str) -> Vec<ProxyUrl> {
    text.lines().filter_map(|l| parse_proxy(l).ok()).collect()
}

/// Open one connection toward `target` through the proxy and send a byte.
/// Success means the proxy accepted the tunnel, not that Psiphon will.
pub async fn quick_test(proxy: &ProxyUrl, target_host: &str, target_port: u16, budget: Duration) -> bool {
    let work = async {
        match proxy.kind {
            ProxyKind::Socks5 => {
                let addr = format!("{}:{}", proxy.host, proxy.port).parse()?;
                let mut s = socks5_connect(addr, target_host, target_port).await?;
                s.write_all(b"\x00").await?;
                Ok::<(), anyhow::Error>(())
            }
            ProxyKind::Http => {
                let mut s = TcpStream::connect((proxy.host.as_str(), proxy.port)).await?;
                let req = format!(
                    "CONNECT {target_host}:{target_port} HTTP/1.1\r\nHost: {target_host}:{target_port}\r\n\r\n"
                );
                s.write_all(req.as_bytes()).await?;
                let mut buf = [0u8; 64];
                let n = s.read_some(&mut buf).await?;
                let text = String::from_utf8_lossy(&buf[..n]);
                if text.starts_with("HTTP/1.") && text.contains(" 200") {
                    Ok(())
                } else {
                    Err(anyhow!("http proxy status {text}"))
                }
            }
        }
    };
    timeout(budget, work).await.ok().and_then(|r| r.ok()).is_some()
}

trait ReadSome {
    async fn read_some(&mut self, buf: &mut [u8]) -> Result<usize>;
}

impl ReadSome for TcpStream {
    async fn read_some(&mut self, buf: &mut [u8]) -> Result<usize> {
        use tokio::io::AsyncReadExt;
        let n = self.read(buf).await?;
        if n == 0 {
            return Err(anyhow!("eof"));
        }
        Ok(n)
    }
}

pub fn onward_for(proxy: &ProxyUrl) -> Result<Onward> {
    match proxy.kind {
        ProxyKind::Socks5 => {
            let addr = format!("{}:{}", proxy.host, proxy.port).parse()?;
            Ok(Onward::Socks5(addr))
        }
        ProxyKind::Http => Err(anyhow!("http proxies are passed to Psiphon, not to the TCP relay")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_kinds_and_skips_comments() {
        let list = parse_list("# c\nhttp://1.2.3.4:8080\nsocks5://user:pw@10.0.0.1:1080\nnope\n");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].kind, ProxyKind::Http);
        assert_eq!(list[0].port, 8080);
        assert_eq!(list[1].kind, ProxyKind::Socks5);
        assert_eq!(list[1].host, "10.0.0.1");
    }
}
