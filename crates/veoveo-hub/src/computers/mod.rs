//! Owner-scoped browser admission and native Computer lifecycle.
//! Browser cache namespaces are convenience; this module owns server-side authority.
mod identity;
mod spec;
pub use identity::ComputerId;
pub use spec::ComputerProvisionSpec;
mod session;
pub use session::{ADMISSION_READ_TIMEOUT, BrowserSession, STREAM_ADMISSION_INTERVAL};

mod gateway;
pub(crate) use gateway::connect as connect_gateway;
mod oauth;
mod onboarding;
mod provisioning;
mod proxy;
pub use onboarding::{ComputerOnboarding, ComputerOnboardingContext, ComputerOnboardingStatus};
pub(crate) mod runtime;
pub use runtime::{ComputerRuntime, ComputerService};
