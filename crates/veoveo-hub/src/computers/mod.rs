//! Owner-scoped browser admission and native Computer lifecycle.
//! Browser cache namespaces are convenience; this module owns server-side authority.
mod identity;
mod lore_client;
mod spec;
pub use identity::{ComputerId, is_native_name};
pub use lore_client::{ComputerOwner, LoreRenewal, lore_client, renew_lore_access};
pub use spec::ComputerProvisionSpec;
mod session;
pub use session::{ADMISSION_READ_TIMEOUT, BrowserSession, STREAM_ADMISSION_INTERVAL};
mod version;
pub use version::{
    ComputerFailure, ComputerVersion, ImageChange, NetworkRule, RELEASE_NOTE, UpdateNotice,
    VersionInfo, WhatsNew,
};

mod api;
mod gateway;
pub(crate) use gateway::connect as connect_gateway;
mod lifecycle;
pub use lifecycle::{ComputerStatus, RestartStatus};
mod oauth;
mod pages;
mod provisioning;
mod proxy;
pub(crate) mod runtime;
#[cfg(test)]
mod test_fixture;
pub use runtime::{ComputerRuntime, ComputerService};
