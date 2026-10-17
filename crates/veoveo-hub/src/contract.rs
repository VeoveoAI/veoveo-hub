//! The Hub's own vocabulary: kernel actions and the catalog registry, OAuth and
//! secret values, upstream HTTP settings, authentication outcomes, health reports, the
//! Console bootstrap document and the usage views.
mod actions;
mod authentication;
mod catalog;
mod console;
mod http;
mod identity;
mod oauth;
mod presentation;
mod secrets;
mod usage;

pub use actions::*;
pub use authentication::*;
pub use catalog::*;
pub use console::*;
pub use http::*;
pub use identity::*;
pub use oauth::*;
pub use presentation::*;
pub use secrets::*;
pub use usage::*;
