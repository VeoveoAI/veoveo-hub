//! The installation's MCP Apps host, a web host by MCP Apps `2026-01-26`: each App's
//! direct URL and host page on the issuer's origin, the sandbox proxy on its own origin,
//! the App's policy from its validated declaration, and the bridge, which runs a view's
//! requests as the person through the Hub's ordinary MCP path.
mod admission;
mod bridge;
mod csp;
mod mcp;
mod origins;
mod pages;
mod route;
mod sandbox;
#[cfg(test)]
mod tests;

pub use bridge::AppsService;
pub use csp::{AppPolicy, PolicyError};
pub use origins::{AppOrigins, SandboxOrigin};
pub use route::{AppView, SANDBOX_SLUG};
pub use sandbox::SandboxProxy;
