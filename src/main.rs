use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use ladder::cleanip::{self, EdgeVerdict};
use ladder::config::AppConfig;
use ladder::plan::{ladder, Method};
use ladder::psiphon::{self, Overlay};
use ladder::relay::{self, Onward};
use ladder::upstream::{self, ProxyUrl};
use ladder::windscribe::{self, Proto};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Parser)]
#[command(name = "ladder", about = "Psiphon ladder and Windscribe-through-Cloudflare relay")]
struct Cli {
    /// Config file. Missing file falls back to config.example.toml beside the binary's cwd.
    #[arg(long, default_value = "config.toml")]
    config: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Psiphon: plan, scan clean edges, test proxies, or run the ladder.
    Psiphon {
        #[command(subcommand)]
        cmd: PsiCmd,
    },
    /// Windscribe: print the protocol plan, or open the Cloudflare TCP relay.
    Windscribe {
        #[command(subcommand)]
        cmd: WsCmd,
    },
}

#[derive(Subcommand)]
enum PsiCmd {
    /// Print the rung order and the budget of each, in seconds.
    Plan,
    /// Resolve edges, check TLS (and meek, if configured), write clean-ips.json.
    Scan,
    /// Quick-test every proxy in the list. Does not start Psiphon.
    Proxies,
    /// Walk the ladder. The first rung whose SOCKS port answers stays up.
    Run,
}

#[derive(Subcommand)]
enum WsCmd {
    /// List nodes and the address each protocol would dial.
    Plan,
    /// Listen locally and open each connection through the Cloudflare SOCKS.
    Relay {
        /// wireguard is not TCP. Use openvpn-tcp, stealth or wstunnel.
        #[arg(long, default_value = "openvpn-tcp")]
        proto: String,
        /// Node id from the servers file. Defaults to the first preferred node.
        #[arg(long)]
        node: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = load(&cli.config)?;
    // On SIGINT/SIGTERM the running future is dropped, which also kills the
    // psiphon child (kill_on_drop). The Android app stops us with SIGTERM.
    tokio::select! {
        r = dispatch(cli.cmd, &cfg) => r,
        _ = shutdown() => {
            eprintln!("stopped");
            Ok(())
        }
    }
}

async fn dispatch(cmd: Cmd, cfg: &AppConfig) -> Result<()> {
    match cmd {
        Cmd::Psiphon { cmd } => match cmd {
            PsiCmd::Plan => cmd_plan(cfg),
            PsiCmd::Scan => cmd_scan(cfg).await,
            PsiCmd::Proxies => cmd_proxies(cfg).await,
            PsiCmd::Run => cmd_run(cfg).await,
        },
        Cmd::Windscribe { cmd } => match cmd {
            WsCmd::Plan => cmd_ws_plan(cfg),
            WsCmd::Relay { proto, node } => cmd_ws_relay(cfg, &proto, node.as_deref()).await,
        },
    }
}

async fn shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

fn load(path: &Path) -> Result<AppConfig> {
    if path.exists() {
        return AppConfig::load(path);
    }
    let example = Path::new("config.example.toml");
    if example.exists() {
        eprintln!("no {}, using {}", path.display(), example.display());
        return AppConfig::load(example);
    }
    AppConfig::load(path)
}

fn cmd_plan(cfg: &AppConfig) -> Result<()> {
    let clean = std::fs::read_to_string(clean_path(cfg)).ok().filter(|t| t.len() > 2).is_some();
    let order = ladder(cfg.psiphon.pinned, remembered(cfg), clean);
    println!("pinned: {}", cfg.psiphon.pinned.map(|m| m.to_string()).unwrap_or_else(|| "auto".into()));
    for m in order {
        println!("  {m}  budget {}s", cfg.budgets.get(m));
    }
    Ok(())
}

fn remembered(cfg: &AppConfig) -> Option<Method> {
    let p = cfg.psiphon.data_root.join("last-rung.txt");
    let s = std::fs::read_to_string(p).ok()?;
    Method::parse(s.trim())
}

fn remember(cfg: &AppConfig, m: Method) -> Result<()> {
    std::fs::create_dir_all(&cfg.psiphon.data_root)?;
    std::fs::write(cfg.psiphon.data_root.join("last-rung.txt"), m.as_str())?;
    Ok(())
}

fn clean_path(cfg: &AppConfig) -> PathBuf {
    cfg.psiphon.data_root.join("clean-ips.json")
}

fn load_cidrs(cfg: &AppConfig) -> BTreeMap<String, Vec<String>> {
    let Ok(text) = std::fs::read_to_string(clean_path(cfg)) else {
        return BTreeMap::new();
    };
    let rows: Vec<CleanRow> = serde_json::from_str(&text).unwrap_or_default();
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows.into_iter().filter(|r| r.ok).take(cfg.psiphon.cleanip.real_top) {
        map.entry(cfg.psiphon.cleanip.provider_id.clone())
            .or_default()
            .push(cleanip::cidr_of(&row.ip));
    }
    map
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CleanRow {
    ip: String,
    ok: bool,
    note: String,
}

async fn cmd_scan(cfg: &AppConfig) -> Result<()> {
    let c = &cfg.psiphon.cleanip;
    let mut seeds = cleanip::names_from_templates(&c.name_templates, c.name_count);
    if !c.user_file.is_empty() {
        if let Ok(text) = std::fs::read_to_string(&c.user_file) {
            for line in text.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    seeds.push(line.to_string());
                }
            }
        }
    }
    if seeds.is_empty() {
        return Err(anyhow!(
            "cleanip.name_templates is empty. Add the fronting names this line should resolve, or a user_file of addresses."
        ));
    }
    eprintln!("resolving {} names", seeds.len());
    let found = cleanip::resolve_names(&seeds).await;
    let mut cand = Vec::new();
    for ip in &found {
        if ip.contains(':') {
            if let Ok(n) = cleanip::expand_v6_cluster(ip) {
                cand.extend(n);
            }
        } else if let Ok(n) = cleanip::expand_v4(ip) {
            cand.extend(n);
        }
        cand.push(ip.clone());
    }
    cand.sort();
    cand.dedup();
    cand.truncate(c.max_ips);
    eprintln!("checking {} candidates", cand.len());
    let mut rows = Vec::new();
    for ip in cand {
        let der = match cleanip::tls_der(&ip, c.tls_timeout_secs).await {
            Ok(d) => d,
            Err(_) => {
                rows.push(CleanRow { ip, ok: false, note: "no-tls".into() });
                continue;
            }
        };
        if !cleanip::cert_matches(&der, &c.cert_needles) {
            rows.push(CleanRow { ip, ok: false, note: "other-cert".into() });
            continue;
        }
        if c.meek_hosts.is_empty() {
            rows.push(CleanRow { ip, ok: true, note: "tls".into() });
            continue;
        }
        let mut verdict = EdgeVerdict::Silent;
        for host in &c.meek_hosts {
            verdict = cleanip::edge_http(&ip, host, c.edge_timeout_secs).await;
            if verdict == EdgeVerdict::Origin || verdict == EdgeVerdict::Other {
                break;
            }
        }
        let ok = verdict != EdgeVerdict::Silent && verdict != EdgeVerdict::NoTls;
        rows.push(CleanRow {
            ip,
            ok,
            note: format!("{verdict:?}").to_ascii_lowercase(),
        });
    }
    let proved = rows.iter().filter(|r| r.ok).count();
    std::fs::create_dir_all(&cfg.psiphon.data_root)?;
    std::fs::write(clean_path(cfg), serde_json::to_vec_pretty(&rows)?)?;
    println!("{proved} edges kept, {} dropped, written to {}", rows.len() - proved, clean_path(cfg).display());
    Ok(())
}

async fn cmd_proxies(cfg: &AppConfig) -> Result<()> {
    let text = std::fs::read_to_string(&cfg.psiphon.upstream.list_file)
        .with_context(|| format!("reading {}", cfg.psiphon.upstream.list_file.display()))?;
    let list = upstream::parse_list(&text);
    if list.is_empty() {
        return Err(anyhow!("no proxies in the list"));
    }
    let budget = Duration::from_secs(cfg.psiphon.upstream.quick_timeout_secs);
    for p in &list {
        let ok = upstream::quick_test(p, &cfg.psiphon.upstream.quick_host, cfg.psiphon.upstream.quick_port, budget).await;
        println!("{}  {}", if ok { "ok " } else { "fail" }, p.raw);
    }
    Ok(())
}

async fn cmd_run(cfg: &AppConfig) -> Result<()> {
    let base_text = std::fs::read_to_string(&cfg.psiphon.base_config).unwrap_or_else(|_| "{}".into());
    let base: serde_json::Value = serde_json::from_str(&base_text).context("psiphon base json")?;
    if base.get("SponsorId").is_none() {
        eprintln!("base config has no SponsorId. The official core will not connect until you add the config from your own psiphon-tunnel-core build.");
    }
    let cidrs = load_cidrs(cfg);
    let has_clean = cidrs.values().any(|v| !v.is_empty());
    let order = ladder(cfg.psiphon.pinned, remembered(cfg), has_clean);
    let proxies = read_proxies(cfg);
    let run_dir = cfg.psiphon.data_root.join("run");
    for method in order {
        let budget = cfg.budgets.get(method);
        let targets: Vec<Option<&ProxyUrl>> = if method == Method::X {
            if proxies.is_empty() {
                eprintln!("X: no proxies, skipping");
                continue;
            }
            proxies.iter().map(Some).collect()
        } else {
            vec![None]
        };
        for proxy in targets {
            let extra = Overlay {
                socks_port: cfg.psiphon.local_socks_port,
                egress_region: cfg.psiphon.egress_region.clone(),
                compartment_id: cfg.psiphon.compartment_id.clone(),
                proxy_url: proxy.map(|p| p.raw.clone()).unwrap_or_default(),
                cidrs: cidrs.clone(),
                budget_secs: budget,
                data_dir: cfg.psiphon.data_root.join(method.as_str()),
            };
            if method == Method::E && extra.cidrs.is_empty() {
                eprintln!("E: no clean IPs yet, run `ladder psiphon scan` first");
                continue;
            }
            let cfg_json = match psiphon::overlay(&base, method, &extra) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{method}: {e}");
                    continue;
                }
            };
            let path = psiphon::write_config(&run_dir, method, &cfg_json)?;
            let label = proxy.map(|p| p.raw.as_str()).unwrap_or("-");
            eprintln!("trying {method} for {budget}s ({label})");
            match psiphon::spawn_until_socks(&cfg.psiphon.binary, &path, Duration::from_secs(budget)).await {
                Ok(Some(mut child)) => {
                    if method != Method::P {
                        let _ = remember(cfg, method);
                    }
                    eprintln!("up. local socks 127.0.0.1:{}", cfg.psiphon.local_socks_port);
                    let status = child.wait().await?;
                    println!("psiphon exited {status}");
                    return Ok(());
                }
                Ok(None) => eprintln!("{method} did not answer within {budget}s"),
                Err(e) => {
                    eprintln!("{method}: {e}");
                    eprintln!("config left at {}", path.display());
                    return Err(e);
                }
            }
            let _ = extra;
        }
    }
    Err(anyhow!("no method connected"))
}

fn read_proxies(cfg: &AppConfig) -> Vec<ProxyUrl> {
    std::fs::read_to_string(&cfg.psiphon.upstream.list_file)
        .map(|t| upstream::parse_list(&t))
        .unwrap_or_default()
}

fn cmd_ws_plan(cfg: &AppConfig) -> Result<()> {
    let text = std::fs::read_to_string(&cfg.windscribe.servers_file)
        .with_context(|| format!("reading {}", cfg.windscribe.servers_file.display()))?;
    let nodes = windscribe::sort_nodes(windscribe::load_nodes(&text)?, &cfg.windscribe.prefer_countries);
    for line in windscribe::plan_lines(&nodes) {
        println!("{line}");
    }
    println!("tcp protocols dial {} and leave via socks {}", cfg.windscribe.listen, cfg.windscribe.socks);
    Ok(())
}

async fn cmd_ws_relay(cfg: &AppConfig, proto_s: &str, node_id: Option<&str>) -> Result<()> {
    let proto = Proto::parse(proto_s).ok_or_else(|| anyhow!("unknown protocol {proto_s}"))?;
    if !proto.uses_tcp_relay() {
        return Err(anyhow!(
            "{proto_s} is UDP. Keep a single Cloudflare WARP session and send WireGuard or OpenVPN UDP through it. This relay is the TCP path (OpenVPN TCP, Stealth, WStunnel)."
        ));
    }
    let text = std::fs::read_to_string(&cfg.windscribe.servers_file)?;
    let nodes = windscribe::sort_nodes(windscribe::load_nodes(&text)?, &cfg.windscribe.prefer_countries);
    let node = if let Some(id) = node_id {
        nodes.into_iter().find(|n| n.id == id).ok_or_else(|| anyhow!("no node {id}"))?
    } else {
        nodes.into_iter().next().ok_or_else(|| anyhow!("no servers"))?
    };
    let (host, port) = windscribe::endpoint(&node, proto).ok_or_else(|| anyhow!("node has no endpoint for {proto_s}"))?;
    let socks: std::net::SocketAddr = cfg.windscribe.socks.parse().context("windscribe.socks")?;
    let listen: std::net::SocketAddr = cfg.windscribe.listen.parse().context("windscribe.listen")?;
    let local = relay::serve(listen, Onward::Socks5(socks), host.to_string(), port).await?;
    println!(
        "relay {} -> {}:{} through socks {}",
        local, host, port, cfg.windscribe.socks
    );
    println!("point OpenVPN/Stealth/WStunnel at {local}. certificate check stays end to end.");
    let _ = json!({"ok": true});
    tokio::signal::ctrl_c().await.ok();
    Ok(())
}
