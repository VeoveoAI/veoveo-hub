//! Authenticated owner operations and native source adapters. Generic activation
//! retains generation/build state; native/OpenShell operations stay at composition.
#[path = "activation/definition_source.rs"]
mod definition_source;
#[path = "activation/lore.rs"]
mod lore;
#[path = "activation/lore_file.rs"]
mod lore_file;
#[path = "activation/native.rs"]
mod native;
#[path = "activation/routes.rs"]
mod routes;
#[path = "activation/source.rs"]
mod source;
#[path = "activation/types.rs"]
mod types;
pub(crate) use routes::ActivationService;
