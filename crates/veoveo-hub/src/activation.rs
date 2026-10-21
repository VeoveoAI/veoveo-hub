//! Source activation is Hub-owned desired/serving state, not native runtime inventory.
//! Native adapters materialize immutable bytes and run workers; routing publication
//! checks the accepted generation in the same transaction as the control revision.
mod types;
pub use types::*;
mod environment;
pub use environment::EnvironmentPublication;
pub(crate) use environment::PublicationGuard;
mod store;
pub use store::{ActivationPlacement, ActivationPublication, ActivationStore};

#[cfg(test)]
mod tests;
