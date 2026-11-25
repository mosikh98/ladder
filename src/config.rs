use crate::budget::Budgets;
use crate::plan::Method;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub budgets: Budgets,
    pub psiphon: PsiphonCfg,
    pub windscribe: WindscribeCfg,
}

#[derive(Clone, Debug)]
pub struct PsiphonCfg {
    pub binary: String,
    pub base_config: PathBuf,
    pub data_root: PathBuf,
    pub local_socks_port: u16,
    pub pinned: Option<Method>,
    pub egress_region: String,
    pub compartment_id: String,
    pub cleanip: CleanIpCfg,
    pub upstream: UpstreamCfg,
}

#[derive(Clone, Debug)]
pub struct CleanIpCfg {
    pub max_ips: usize,
    pub tls_timeout_secs: u64,
    pub edge_timeout_secs: u64,
    pub real_top: usize,
    pub name_templates: Vec<String>,
    pub name_count: u32,
    pub cert_needles: Vec<String>,
    pub meek_hosts: Vec<String>,
    pub user_file: String,
    pub provider_id: String,
}

#[derive(Clone, Debug)]
pub struct UpstreamCfg {
    pub list_file: PathBuf,
    pub quick_host: String,
    pub quick_port: u16,
    pub quick_timeout_secs: u64,
}

#[derive(Clone, Debug)]
pub struct WindscribeCfg {
    pub socks: String,
    pub listen: String,
    pub servers_file: PathBuf,
    pub prefer_countries: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Raw {
    #[serde(default)]
    budgets: HashMap<String, u64>,
    #[serde(default)]
    psiphon: RawPsiphon,
    #[serde(default)]
    windscribe: RawWs,
}

#[derive(Debug, Deserialize)]
struct RawPsiphon {
    #[serde(default = "default_bin")]
    binary: String,
    #[serde(default = "default_base")]
    base_config: String,
    #[serde(default = "default_data")]
    data_root: String,
    #[serde(default = "default_socks")]
    local_socks_port: u16,
    #[serde(default)]
    pinned: String,
    #[serde(default)]
    egress_region: String,
    #[serde(default)]
    compartment_id: String,
    #[serde(default)]
    cleanip: RawClean,
    #[serde(default)]
    upstream: RawUp,
}

impl Default for RawPsiphon {
    fn default() -> Self {
        Self {
            binary: default_bin(),
            base_config: default_base(),
            data_root: default_data(),
            local_socks_port: default_socks(),
            pinned: String::new(),
            egress_region: String::new(),
            compartment_id: String::new(),
            cleanip: RawClean::default(),
            upstream: RawUp::default(),
        }
    }
}

fn default_bin() -> String {
    "psiphon-tunnel-core".into()
}
fn default_base() -> String {
    "psiphon.base.json".into()
}
fn default_data() -> String {
    "data/psiphon".into()
}
fn default_socks() -> u16 {
    1080
}

#[derive(Debug, Deserialize)]
struct RawClean {
    #[serde(default = "d_max")]
    max_ips: usize,
    #[serde(default = "d_tls")]
    tls_timeout_secs: u64,
    #[serde(default = "d_edge")]
    edge_timeout_secs: u64,
    #[serde(default = "d_top")]
    real_top: usize,
    #[serde(default)]
    name_templates: Vec<String>,
    #[serde(default = "d_names")]
    name_count: u32,
    #[serde(default = "d_needles")]
    cert_needles: Vec<String>,
    #[serde(default)]
    meek_hosts: Vec<String>,
    #[serde(default)]
    user_file: String,
    #[serde(default = "d_provider")]
    provider_id: String,
}

impl Default for RawClean {
    fn default() -> Self {
        Self {
            max_ips: d_max(),
            tls_timeout_secs: d_tls(),
            edge_timeout_secs: d_edge(),
            real_top: d_top(),
            name_templates: vec![],
            name_count: d_names(),
            cert_needles: d_needles(),
            meek_hosts: vec![],
            user_file: String::new(),
            provider_id: d_provider(),
        }
    }
}

fn d_max() -> usize {
    400
}
fn d_tls() -> u64 {
    4
}
fn d_edge() -> u64 {
    20
}
fn d_top() -> usize {
    8
}
fn d_names() -> u32 {
    24
}
fn d_needles() -> Vec<String> {
    vec!["akamai".into()]
}
fn d_provider() -> String {
    "BACEC04E64813F2B".into()
}

#[derive(Debug, Deserialize)]
struct RawUp {
    #[serde(default = "d_list")]
    list_file: String,
    #[serde(default = "d_host")]
    quick_host: String,
    #[serde(default = "d_port")]
    quick_port: u16,
    #[serde(default = "d_qt")]
    quick_timeout_secs: u64,
}

impl Default for RawUp {
    fn default() -> Self {
        Self {
            list_file: d_list(),
            quick_host: d_host(),
            quick_port: d_port(),
            quick_timeout_secs: d_qt(),
        }
    }
}

fn d_list() -> String {
    "proxies.txt".into()
}
fn d_host() -> String {
    "www.example.com".into()
}
fn d_port() -> u16 {
    443
}
fn d_qt() -> u64 {
    8
}

#[derive(Debug, Deserialize)]
struct RawWs {
    #[serde(default = "d_warp")]
    socks: String,
    #[serde(default = "d_listen")]
    listen: String,
    #[serde(default = "d_servers")]
    servers_file: String,
    #[serde(default = "d_countries")]
    prefer_countries: Vec<String>,
}

impl Default for RawWs {
    fn default() -> Self {
        Self {
            socks: d_warp(),
            listen: d_listen(),
            servers_file: d_servers(),
            prefer_countries: d_countries(),
        }
    }
}

fn d_warp() -> String {
    "127.0.0.1:40000".into()
}
fn d_listen() -> String {
    "127.0.0.1:1194".into()
}
fn d_servers() -> String {
    "examples/servers.example.json".into()
}
fn d_countries() -> Vec<String> {
    vec!["TR".into(), "AE".into(), "DE".into(), "NL".into()]
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let raw: Raw = toml::from_str(&text).context("parsing config")?;
        let mut budgets = Budgets::default();
        budgets.apply_file(&raw.budgets);
        let dir = path.parent().unwrap_or(Path::new("."));
        let resolve = |p: &str| {
            let pb = PathBuf::from(p);
            if pb.is_absolute() {
                pb
            } else {
                dir.join(pb)
            }
        };
        Ok(Self {
            budgets,
            psiphon: PsiphonCfg {
                binary: raw.psiphon.binary,
                base_config: resolve(&raw.psiphon.base_config),
                data_root: resolve(&raw.psiphon.data_root),
                local_socks_port: raw.psiphon.local_socks_port,
                pinned: Method::parse(&raw.psiphon.pinned),
                egress_region: raw.psiphon.egress_region,
                compartment_id: raw.psiphon.compartment_id,
                cleanip: CleanIpCfg {
                    max_ips: raw.psiphon.cleanip.max_ips,
                    tls_timeout_secs: raw.psiphon.cleanip.tls_timeout_secs,
                    edge_timeout_secs: raw.psiphon.cleanip.edge_timeout_secs,
                    real_top: raw.psiphon.cleanip.real_top,
                    name_templates: raw.psiphon.cleanip.name_templates,
                    name_count: raw.psiphon.cleanip.name_count,
                    cert_needles: raw.psiphon.cleanip.cert_needles,
                    meek_hosts: raw.psiphon.cleanip.meek_hosts,
                    user_file: raw.psiphon.cleanip.user_file,
                    provider_id: raw.psiphon.cleanip.provider_id,
                },
                upstream: UpstreamCfg {
                    list_file: resolve(&raw.psiphon.upstream.list_file),
                    quick_host: raw.psiphon.upstream.quick_host,
                    quick_port: raw.psiphon.upstream.quick_port,
                    quick_timeout_secs: raw.psiphon.upstream.quick_timeout_secs,
                },
            },
            windscribe: WindscribeCfg {
                socks: raw.windscribe.socks,
                listen: raw.windscribe.listen,
                servers_file: resolve(&raw.windscribe.servers_file),
                prefer_countries: raw.windscribe.prefer_countries,
            },
        })
    }

    pub fn example() -> Self {
        Self::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("config.example.toml"))
            .expect("example config")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_file_loads_and_keeps_seconds() {
        let c = AppConfig::example();
        assert_eq!(c.budgets.get(crate::plan::Method::A), 60);
        assert_eq!(c.budgets.get(crate::plan::Method::X), 45);
        assert!(c.psiphon.pinned.is_none());
        assert_eq!(c.windscribe.socks, "127.0.0.1:40000");
    }
}
