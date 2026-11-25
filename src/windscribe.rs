//! Windscribe server choice and which address each protocol dials.
//!
//! On a line that blackholes every Windscribe address, the TCP protocols are
//! not dialed directly. They dial the loopback relay, and the relay opens the
//! real address from inside Cloudflare (a WARP SOCKS port, or a Worker).
//! WireGuard is UDP: this crate records the endpoint (`ip3`) but does not ship
//! a second WARP key. Run one WARP/SOCKS session and keep it for every TCP
//! protocol. A second session on the same key takes the first one.

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Node {
    pub id: String,
    pub country: String,
    pub city: String,
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub ip2: String,
    #[serde(default)]
    pub ip3: String,
    #[serde(default)]
    pub wg_port: u16,
    #[serde(default)]
    pub ovpn_tcp: u16,
    #[serde(default)]
    pub ovpn_udp: u16,
    #[serde(default)]
    pub stealth: u16,
    #[serde(default)]
    pub wstunnel: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proto {
    WireGuard,
    OpenVpnTcp,
    OpenVpnUdp,
    Stealth,
    Wstunnel,
}

impl Proto {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "wireguard" | "wg" => Some(Self::WireGuard),
            "openvpn-tcp" | "tcp" => Some(Self::OpenVpnTcp),
            "openvpn-udp" | "udp" => Some(Self::OpenVpnUdp),
            "stealth" => Some(Self::Stealth),
            "wstunnel" | "ws" => Some(Self::Wstunnel),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::WireGuard => "wireguard",
            Self::OpenVpnTcp => "openvpn-tcp",
            Self::OpenVpnUdp => "openvpn-udp",
            Self::Stealth => "stealth",
            Self::Wstunnel => "wstunnel",
        }
    }

    /// TCP protocols can use the loopback relay. UDP needs SOCKS UDP ASSOCIATE
    /// inside the same WARP session, which this crate does not open itself.
    pub fn uses_tcp_relay(self) -> bool {
        matches!(self, Self::OpenVpnTcp | Self::Stealth | Self::Wstunnel)
    }
}

/// Official address pick: WireGuard and Stealth use `ip3`, WStunnel uses `ip`,
/// OpenVPN uses `ip2`.
pub fn endpoint(node: &Node, proto: Proto) -> Option<(&str, u16)> {
    let (host, port) = match proto {
        Proto::WireGuard => (node.ip3.as_str(), node.wg_port),
        Proto::Stealth => (node.ip3.as_str(), node.stealth),
        Proto::Wstunnel => (node.ip.as_str(), node.wstunnel),
        Proto::OpenVpnTcp => (node.ip2.as_str(), node.ovpn_tcp),
        Proto::OpenVpnUdp => (node.ip2.as_str(), node.ovpn_udp),
    };
    if host.is_empty() || port == 0 {
        None
    } else {
        Some((host, port))
    }
}

pub const TRY_ORDER: [Proto; 5] = [
    Proto::WireGuard,
    Proto::OpenVpnTcp,
    Proto::Stealth,
    Proto::Wstunnel,
    Proto::OpenVpnUdp,
];

pub fn sort_nodes(nodes: Vec<Node>, prefer: &[String]) -> Vec<Node> {
    let mut nodes = nodes;
    nodes.sort_by_key(|n| {
        prefer
            .iter()
            .position(|c| c.eq_ignore_ascii_case(&n.country))
            .unwrap_or(100)
    });
    nodes
}

pub fn load_nodes(text: &str) -> Result<Vec<Node>> {
    serde_json::from_str(text).context("servers json")
}

pub fn plan_lines(nodes: &[Node]) -> Vec<String> {
    let mut lines = Vec::new();
    for n in nodes {
        for p in TRY_ORDER {
            if let Some((h, port)) = endpoint(n, p) {
                let via = if p.uses_tcp_relay() {
                    "cloudflare-socks"
                } else if p == Proto::WireGuard {
                    "warp-udp"
                } else {
                    "warp-udp-associate"
                };
                lines.push(format!(
                    "{} {} {} {}:{} via {via}",
                    n.country, n.city, p.name(), h, port
                ));
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node() -> Node {
        Node {
            id: "1".into(),
            country: "TR".into(),
            city: "Istanbul".into(),
            ip: "1.1.1.1".into(),
            ip2: "2.2.2.2".into(),
            ip3: "3.3.3.3".into(),
            wg_port: 443,
            ovpn_tcp: 1194,
            ovpn_udp: 1194,
            stealth: 443,
            wstunnel: 8443,
        }
    }

    #[test]
    fn address_pick_matches_official_fields() {
        let n = node();
        assert_eq!(endpoint(&n, Proto::WireGuard), Some(("3.3.3.3", 443)));
        assert_eq!(endpoint(&n, Proto::Stealth), Some(("3.3.3.3", 443)));
        assert_eq!(endpoint(&n, Proto::Wstunnel), Some(("1.1.1.1", 8443)));
        assert_eq!(endpoint(&n, Proto::OpenVpnTcp), Some(("2.2.2.2", 1194)));
    }

    #[test]
    fn preferred_country_sorts_first() {
        let nodes = vec![
            Node {
                country: "US".into(),
                city: "Dallas".into(),
                id: "us".into(),
                ..node()
            },
            Node {
                country: "TR".into(),
                city: "Istanbul".into(),
                id: "tr".into(),
                ..node()
            },
        ];
        let s = sort_nodes(nodes, &["TR".into(), "DE".into()]);
        assert_eq!(s[0].id, "tr");
    }
}
