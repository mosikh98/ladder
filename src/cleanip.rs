//! Clean-edge finder for Psiphon fronting.
//!
//! Candidates are names you list (resolved on this host), the neighbouring
//! addresses in the same edge cluster, and addresses you typed yourself.
//! A candidate stays only when TLS on port 443 presents a certificate that
//! matches the provider. If meek hosts are configured, an edge that completes
//! TLS and then stays silent is dropped. Answering edges are written as /32
//! or /128 for method E.

use anyhow::{anyhow, Result};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use x509_parser::prelude::*;

pub fn expand_v4(ip: &str) -> Result<Vec<String>> {
    let n = parse_v4(ip).ok_or_else(|| anyhow!("not ipv4: {ip}"))?;
    let bits = 24u32;
    let size = 1u32 << (32 - bits);
    let base = (n / size) * size;
    Ok((0..size)
        .filter_map(|k| {
            let v = base.wrapping_add(k);
            let last = v & 255;
            if last == 0 || last == 255 {
                None
            } else {
                Some(fmt_v4(v))
            }
        })
        .collect())
}

pub fn expand_v6_cluster(ip: &str) -> Result<Vec<String>> {
    let g = parse_v6(ip).ok_or_else(|| anyhow!("not ipv6: {ip}"))?;
    let hi = g[7] & 0xff00;
    Ok((0..256)
        .map(|k| {
            let mut x = g;
            x[7] = hi + k as u16;
            compact_v6(&x)
        })
        .collect())
}

pub fn names_from_templates(templates: &[String], count: u32) -> Vec<String> {
    let mut out = Vec::new();
    for t in templates {
        for i in 0..count {
            out.push(t.replace("{n}", &(1000 + i).to_string()));
        }
    }
    out
}

pub fn cidr_of(ip: &str) -> String {
    if ip.contains(':') {
        format!("{ip}/128")
    } else {
        format!("{ip}/32")
    }
}

fn parse_v4(ip: &str) -> Option<u32> {
    let p: Vec<&str> = ip.split('.').collect();
    if p.len() != 4 {
        return None;
    }
    let mut n = 0u32;
    for part in p {
        let o: u32 = part.parse().ok()?;
        if o > 255 {
            return None;
        }
        n = n * 256 + o;
    }
    Some(n)
}

fn fmt_v4(n: u32) -> String {
    format!(
        "{}.{}.{}.{}",
        (n >> 24) & 255,
        (n >> 16) & 255,
        (n >> 8) & 255,
        n & 255
    )
}

fn parse_v6(ip: &str) -> Option<[u16; 8]> {
    let (head, tail) = if let Some((h, t)) = ip.split_once("::") {
        (h, t)
    } else {
        (ip, "")
    };
    let hp: Vec<&str> = if head.is_empty() { vec![] } else { head.split(':').collect() };
    let tp: Vec<&str> = if tail.is_empty() { vec![] } else { tail.split(':').collect() };
    if ip.contains("::") {
        if hp.len() + tp.len() > 8 {
            return None;
        }
    } else if hp.len() != 8 {
        return None;
    }
    let mut g = [0u16; 8];
    for (i, p) in hp.iter().enumerate() {
        g[i] = u16::from_str_radix(p, 16).ok()?;
    }
    let gap = 8 - hp.len() - tp.len();
    for (i, p) in tp.iter().enumerate() {
        g[hp.len() + gap + i] = u16::from_str_radix(p, 16).ok()?;
    }
    Some(g)
}

fn compact_v6(g: &[u16; 8]) -> String {
    let mut best = (0usize, 0usize);
    let mut i = 0;
    while i < 8 {
        if g[i] == 0 {
            let s = i;
            while i < 8 && g[i] == 0 {
                i += 1;
            }
            if i - s > best.1 - best.0 && i - s > 1 {
                best = (s, i);
            }
        } else {
            i += 1;
        }
    }
    let mut parts = Vec::new();
    let mut i = 0;
    let mut skipped = false;
    while i < 8 {
        if !skipped && i == best.0 && best.1 > best.0 {
            parts.push(String::new());
            if best.0 == 0 {
                parts.push(String::new());
            }
            i = best.1;
            skipped = true;
            if i == 8 {
                parts.push(String::new());
            }
            continue;
        }
        parts.push(format!("{:x}", g[i]));
        i += 1;
    }
    parts.join(":")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeVerdict {
    /// Provider certificate, and the meek origin answered.
    Origin,
    /// Provider certificate, something came back.
    Other,
    /// Provider certificate, then silence.
    Silent,
    /// No provider certificate.
    NoTls,
}

struct Capture {
    der: Mutex<Option<Vec<u8>>>,
}

impl std::fmt::Debug for Capture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Capture")
    }
}

impl ServerCertVerifier for Capture {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        *self.der.lock().unwrap() = Some(end_entity.as_ref().to_vec());
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PKCS1_SHA1,
        ]
    }
}

fn install_ring() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

pub fn cert_matches(der: &[u8], needles: &[String]) -> bool {
    let Ok((_, cert)) = X509Certificate::from_der(der) else {
        return false;
    };
    let mut blob = cert.subject().to_string();
    if let Ok(Some(sans)) = cert.subject_alternative_name() {
        for name in &sans.value.general_names {
            blob.push(' ');
            blob.push_str(&name.to_string());
        }
    }
    let blob = blob.to_ascii_lowercase();
    needles.iter().any(|n| blob.contains(&n.to_ascii_lowercase()))
}

pub fn meek_answer(raw: &str) -> EdgeVerdict {
    let status = raw.split("\r\n").next().unwrap_or("");
    let body = raw.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
    if status.starts_with("HTTP/1.") && status.contains(" 404") && body.starts_with("404 page not found")
    {
        EdgeVerdict::Origin
    } else if raw.is_empty() {
        EdgeVerdict::Silent
    } else {
        EdgeVerdict::Other
    }
}

fn random_sni() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(1);
    let alphabet = b"abcdefghijklmnopqrstuvwxyz";
    let mut s = String::new();
    let mut x = n | 1;
    for _ in 0..5 {
        s.push(alphabet[(x % 26) as usize] as char);
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
    }
    s.push_str(".com");
    s
}

/// TLS to `ip:443` with a random SNI. Returns the end-entity DER when the
/// handshake finishes. The certificate is not trusted; the caller checks it.
pub async fn tls_der(ip: &str, timeout_secs: u64) -> Result<Vec<u8>> {
    install_ring();
    let addr = resolve_443(ip).await?;
    let cap = Arc::new(Capture {
        der: Mutex::new(None),
    });
    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(cap.clone())
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let sni = random_sni();
    let name = ServerName::try_from(sni.clone()).map_err(|e| anyhow!("sni: {e}"))?;
    let stream = timeout(Duration::from_secs(timeout_secs), TcpStream::connect(addr))
        .await
        .map_err(|_| anyhow!("timeout"))??;
    let tls = timeout(
        Duration::from_secs(timeout_secs),
        connector.connect(name, stream),
    )
    .await
    .map_err(|_| anyhow!("tls timeout"))??;
    drop(tls);
    let der = cap.der.lock().unwrap().clone();
    der.ok_or_else(|| anyhow!("no certificate"))
}

async fn resolve_443(ip: &str) -> Result<SocketAddr> {
    if let Ok(ip) = ip.parse() {
        return Ok(SocketAddr::new(ip, 443));
    }
    let mut it = tokio::net::lookup_host((ip, 443)).await?;
    it.next().ok_or_else(|| anyhow!("no address for {ip}"))
}

/// One meek-style request through an edge that already has the provider cert.
pub async fn edge_http(ip: &str, host: &str, timeout_secs: u64) -> EdgeVerdict {
    install_ring();
    let Ok(addr) = resolve_443(ip).await else {
        return EdgeVerdict::NoTls;
    };
    let cap = Arc::new(Capture {
        der: Mutex::new(None),
    });
    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(cap)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let sni = random_sni();
    let Ok(name) = ServerName::try_from(sni) else {
        return EdgeVerdict::NoTls;
    };
    let Ok(Ok(stream)) = timeout(Duration::from_secs(timeout_secs), TcpStream::connect(addr)).await else {
        return EdgeVerdict::NoTls;
    };
    let Ok(Ok(mut tls)) =
        timeout(Duration::from_secs(timeout_secs), connector.connect(name, stream)).await
    else {
        return EdgeVerdict::NoTls;
    };
    let req = format!(
        "GET / HTTP/1.1\r\nHost: {host}\r\nUser-Agent: Mozilla/5.0\r\nAccept: */*\r\nConnection: close\r\n\r\n"
    );
    if tls.write_all(req.as_bytes()).await.is_err() {
        return EdgeVerdict::Silent;
    }
    let mut buf = vec![0u8; 2048];
    let mut got = Vec::new();
    let read = timeout(Duration::from_secs(timeout_secs), async {
        loop {
            let n = tls.read(&mut buf).await.unwrap_or(0);
            if n == 0 {
                break;
            }
            got.extend_from_slice(&buf[..n]);
            if got.len() > 4096 {
                break;
            }
            if got.windows(4).any(|w| w == b"\r\n\r\n") {
                let text = String::from_utf8_lossy(&got);
                if meek_answer(&text) == EdgeVerdict::Origin {
                    break;
                }
            }
        }
    })
    .await;
    if read.is_err() && got.is_empty() {
        return EdgeVerdict::Silent;
    }
    if got.is_empty() {
        EdgeVerdict::Silent
    } else {
        meek_answer(&String::from_utf8_lossy(&got))
    }
}

pub async fn resolve_names(names: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for name in names {
        if let Ok(iter) = tokio::net::lookup_host((name.as_str(), 443)).await {
            for addr in iter {
                let ip = addr.ip().to_string();
                if !out.iter().any(|e: &String| e == &ip) {
                    out.push(ip);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v4_neighbours_skip_network_and_broadcast() {
        let v = expand_v4("203.0.113.10").unwrap();
        assert_eq!(v.len(), 254);
        assert!(!v.iter().any(|s| s.ends_with(".0") || s.ends_with(".255")));
        assert!(v.iter().any(|s| s == "203.0.113.10"));
    }

    #[test]
    fn v6_cluster_is_256_wide() {
        let v = expand_v6_cluster("2001:db8::10").unwrap();
        assert_eq!(v.len(), 256);
        assert!(v.iter().all(|s| s.starts_with("2001:db8::")));
    }

    #[test]
    fn meek_404_is_origin() {
        let raw = "HTTP/1.1 404 Not Found\r\nContent-Length: 19\r\n\r\n404 page not found\n";
        assert_eq!(meek_answer(raw), EdgeVerdict::Origin);
        assert_eq!(meek_answer(""), EdgeVerdict::Silent);
    }

    #[test]
    fn cidr_width() {
        assert_eq!(cidr_of("203.0.113.5"), "203.0.113.5/32");
        assert_eq!(cidr_of("2001:db8::1"), "2001:db8::1/128");
    }

    #[test]
    fn templates_substitute_n() {
        let n = names_from_templates(&["a{n}.example.net".into()], 2);
        assert_eq!(n, vec!["a1000.example.net", "a1001.example.net"]);
    }
}
