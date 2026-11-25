//! Which Psiphon method runs, and in which order.
//!
//! Automatic order is fronting, proved clean edges, direct, then Conduit.
//! Personal Conduit is never automatic. The proxy rung is a pin: each proxy
//! gets its own budget, then the automatic rungs if none of them carry.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Method {
    /// Domain fronting. The TLS session ends on a CDN edge.
    A,
    /// Fronting, dialed at clean edge addresses proved on this line.
    E,
    /// Every protocol, straight to Psiphon's own servers.
    D,
    /// Conduit volunteers over WebRTC.
    C,
    /// One friend's Conduit station.
    P,
    /// HTTP or SOCKS5 upstream in front of Psiphon.
    X,
}

impl Method {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "A" => Some(Self::A),
            "E" => Some(Self::E),
            "D" => Some(Self::D),
            "C" => Some(Self::C),
            "P" => Some(Self::P),
            "X" => Some(Self::X),
            "" | "AUTO" => None,
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::E => "E",
            Self::D => "D",
            Self::C => "C",
            Self::P => "P",
            Self::X => "X",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::A => "domain fronting",
            Self::E => "clean IP",
            Self::D => "direct",
            Self::C => "conduit",
            Self::P => "personal conduit",
            Self::X => "upstream proxy",
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.as_str(), self.title())
    }
}

impl FromStr for Method {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        Self::parse(s).ok_or(())
    }
}

/// `pinned` wins. Empty pin uses the ladder, with `remembered` moved first.
/// Method E is skipped when this line has no proved clean addresses.
/// Method P is included only when it is the pin.
pub fn ladder(pinned: Option<Method>, remembered: Option<Method>, has_clean: bool) -> Vec<Method> {
    if pinned == Some(Method::X) {
        let mut v = vec![Method::X, Method::A, Method::D, Method::C];
        if has_clean {
            v.insert(2, Method::E);
        }
        return v;
    }
    if let Some(m) = pinned {
        return vec![m];
    }
    let mut order = vec![Method::A, Method::E, Method::D, Method::C];
    if !has_clean {
        order.retain(|m| *m != Method::E);
    }
    if let Some(r) = remembered {
        if let Some(i) = order.iter().position(|m| *m == r) {
            let m = order.remove(i);
            order.insert(0, m);
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_skips_clean_and_personal() {
        assert_eq!(
            ladder(None, None, false),
            vec![Method::A, Method::D, Method::C]
        );
    }

    #[test]
    fn remembered_rung_moves_first() {
        assert_eq!(
            ladder(None, Some(Method::C), true),
            vec![Method::C, Method::A, Method::E, Method::D]
        );
    }

    #[test]
    fn pin_is_exclusive_except_proxy() {
        assert_eq!(ladder(Some(Method::D), Some(Method::A), true), vec![Method::D]);
        assert_eq!(
            ladder(Some(Method::X), None, true),
            vec![Method::X, Method::A, Method::E, Method::D, Method::C]
        );
    }

    #[test]
    fn personal_only_when_asked() {
        assert!(!ladder(None, Some(Method::P), true).contains(&Method::P));
        assert_eq!(ladder(Some(Method::P), None, false), vec![Method::P]);
    }
}
