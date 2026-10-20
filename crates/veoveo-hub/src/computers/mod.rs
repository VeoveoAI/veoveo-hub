//! Owner-scoped browser admission and native Computer lifecycle.
//! Browser cache namespaces are convenience; this module owns server-side authority.
mod identity;
mod spec;
pub use identity::ComputerId;
pub use spec::ComputerProvisionSpec;
mod session;
pub use session::{ADMISSION_READ_TIMEOUT, BrowserSession, STREAM_ADMISSION_INTERVAL};

mod gateway;
mod oauth;
mod provisioning;
mod proxy;
pub(crate) mod runtime;
pub use runtime::{ComputerRuntime, ComputerService};
