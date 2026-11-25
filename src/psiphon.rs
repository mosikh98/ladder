//! Build a Psiphon config for one rung and, when the official binary is on
//! PATH, run it until the local SOCKS port answers or the budget runs out.

use crate::plan::Method;
use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::time::timeout;

pub const FRONTED: &[&str] = &["FRONTED-MEEK-OSSH", "FRONTED-MEEK-HTTP-OSSH"];
pub const INPROXY: &[&str] = &["INPROXY-WEBRTC-OSSH"];

#[derive(Clone, Debug)]
pub struct Overlay {
    pub socks_port: u16,
    pub egress_region: String,
    pub compartment_id: String,
    pub proxy_url: String,
    /// provider id -> CIDRs, method E only.
    pub cidrs: BTreeMap<String, Vec<String>>,
    pub budget_secs: u64,
    pub data_dir: PathBuf,
}

pub fn overlay(base: &Value, method: Method, extra: &Overlay) -> Result<Value> {
    let mut cfg = base.clone();
    let obj = cfg
        .as_object_mut()
        .ok_or_else(|| anyhow!("psiphon base config must be a JSON object"))?;

    obj.insert("LocalSocksProxyPort".into(), json!(extra.socks_port));
    obj.insert(
        "EstablishTunnelTimeoutSeconds".into(),
        json!(extra.budget_secs),
    );
    obj.insert("DataRootDirectory".into(), json!(extra.data_dir.display().to_string()));
    if extra.egress_region.len() == 2 {
        obj.insert("EgressRegion".into(), json!(extra.egress_region));
    } else {
        obj.remove("EgressRegion");
    }

    match method {
        Method::A => {
            obj.insert("LimitTunnelProtocols".into(), json!(FRONTED));
            obj.remove("UpstreamProxyURL");
            obj.remove("DNSResolverPreresolvedIPAddressCIDRs");
            obj.remove("InproxyClientPersonalCompartmentID");
        }
        Method::E => {
            obj.insert("LimitTunnelProtocols".into(), json!(FRONTED));
            obj.insert(
                "DNSResolverPreresolvedIPAddressCIDRs".into(),
                serde_json::to_value(&extra.cidrs)?,
            );
            obj.remove("UpstreamProxyURL");
            obj.remove("InproxyClientPersonalCompartmentID");
        }
        Method::D => {
            obj.remove("LimitTunnelProtocols");
            obj.remove("UpstreamProxyURL");
            obj.remove("DNSResolverPreresolvedIPAddressCIDRs");
            obj.remove("InproxyClientPersonalCompartmentID");
        }
        Method::C => {
            obj.insert("LimitTunnelProtocols".into(), json!(INPROXY));
            obj.remove("UpstreamProxyURL");
            obj.remove("InproxyClientPersonalCompartmentID");
        }
        Method::P => {
            if extra.compartment_id.len() < 8 {
                return Err(anyhow!("method P needs psiphon.compartment_id from the pairing link"));
            }
            obj.insert("LimitTunnelProtocols".into(), json!(INPROXY));
            obj.insert(
                "InproxyClientPersonalCompartmentID".into(),
                json!(extra.compartment_id),
            );
            obj.remove("UpstreamProxyURL");
        }
        Method::X => {
            if extra.proxy_url.is_empty() {
                return Err(anyhow!("method X needs an upstream proxy URL"));
            }
            obj.insert("UpstreamProxyURL".into(), json!(extra.proxy_url));
            obj.remove("LimitTunnelProtocols");
            obj.remove("InproxyClientPersonalCompartmentID");
        }
    }
    Ok(cfg)
}

/// Poll the SOCKS port until it accepts or `budget` elapses.
pub async fn wait_socks(port: u16, budget: Duration) -> bool {
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], port)));
    let deadline = tokio::time::Instant::now() + budget;
    while tokio::time::Instant::now() < deadline {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        if timeout(Duration::from_millis(400).min(left), TcpStream::connect(addr))
            .await
            .ok()
            .and_then(|r| r.ok())
            .is_some()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    false
}

/// Start the official binary. `Some` means the local SOCKS port accepted a
/// connection inside the budget; the child is still running. `None` means this
/// rung did not come up. A missing binary is an error, because no later rung
/// can work without it either.
pub async fn spawn_until_socks(
    binary: &str,
    config_path: &Path,
    budget: Duration,
) -> Result<Option<tokio::process::Child>> {
    let port = read_socks_port(config_path).unwrap_or(1080);
    let mut child = Command::new(binary)
        .arg("-config")
        .arg(config_path)
        .kill_on_drop(true)
        .spawn()
        .with_context(|| {
            format!(
                "starting {binary}. Build it from https://github.com/Psiphon-Labs/psiphon-tunnel-core and put it on PATH, or set psiphon.binary. Config was written to {}",
                config_path.display()
            )
        })?;
    tokio::select! {
        ok = wait_socks(port, budget) => {
            if ok {
                Ok(Some(child))
            } else {
                let _ = child.kill().await;
                Ok(None)
            }
        }
        status = child.wait() => {
            let code = status.ok().and_then(|s| s.code()).unwrap_or(-1);
            eprintln!("{binary} exited ({code}) before the tunnel was up");
            Ok(None)
        }
    }
}

fn read_socks_port(path: &Path) -> Option<u16> {
    let text = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    v.get("LocalSocksProxyPort")?.as_u64().map(|n| n as u16)
}

/// Write `cfg` and return the path. Callers run the binary themselves so a
/// missing binary still leaves a config they can inspect.
pub fn write_config(dir: &Path, method: Method, cfg: &Value) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("method-{}.json", method.as_str()));
    std::fs::write(&path, serde_json::to_vec_pretty(cfg)?)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn extra() -> Overlay {
        Overlay {
            socks_port: 1080,
            egress_region: String::new(),
            compartment_id: "compartment-id-0123456789".into(),
            proxy_url: "socks5://127.0.0.1:9050".into(),
            cidrs: BTreeMap::from([("PROV".into(), vec!["192.0.2.10/32".into()])]),
            budget_secs: 12,
            data_dir: PathBuf::from("data/t"),
        }
    }

    #[test]
    fn fronting_drops_proxy_and_limits_protocols() {
        let cfg = overlay(&json!({"SponsorId": "x"}), Method::A, &extra()).unwrap();
        assert!(cfg.get("UpstreamProxyURL").is_none());
        assert_eq!(cfg["LimitTunnelProtocols"][0], "FRONTED-MEEK-OSSH");
        assert_eq!(cfg["EstablishTunnelTimeoutSeconds"], 12);
        assert_eq!(cfg["SponsorId"], "x");
    }

    #[test]
    fn clean_ip_sets_preresolved_cidrs() {
        let cfg = overlay(&json!({}), Method::E, &extra()).unwrap();
        assert_eq!(cfg["DNSResolverPreresolvedIPAddressCIDRs"]["PROV"][0], "192.0.2.10/32");
    }

    #[test]
    fn proxy_sets_upstream_url() {
        let cfg = overlay(&json!({}), Method::X, &extra()).unwrap();
        assert_eq!(cfg["UpstreamProxyURL"], "socks5://127.0.0.1:9050");
        assert!(cfg.get("LimitTunnelProtocols").is_none());
    }

    #[test]
    fn personal_requires_compartment() {
        let mut e = extra();
        e.compartment_id.clear();
        assert!(overlay(&json!({}), Method::P, &e).is_err());
    }
}
