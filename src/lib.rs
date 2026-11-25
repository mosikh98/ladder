//! Orchestrator for a Psiphon method ladder and a Windscribe TCP relay that
//! leaves through a Cloudflare SOCKS endpoint. Tunnel cryptography stays in
//! the official binaries; this crate decides the order, the budget, and the path.

pub mod budget;
pub mod cleanip;
pub mod config;
pub mod plan;
pub mod psiphon;
pub mod relay;
pub mod upstream;
pub mod windscribe;

pub use budget::Budgets;
pub use plan::{ladder, Method};
