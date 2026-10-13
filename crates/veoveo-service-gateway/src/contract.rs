//! The Gateway's own vocabulary: kernel actions and the catalog registry, OAuth and
//! secret values, upstream HTTP settings, authentication outcomes, health reports and
//! the Console bootstrap document.
mod actions;
mod authentication;
mod catalog;
mod console;
mod http;
mod identity;
mod oauth;
mod presentation;
mod secrets;

pub use actions::*;
pub use authentication::*;
pub use catalog::*;
pub use console::*;
pub use http::*;
pub use identity::*;
pub use oauth::*;
pub use presentation::*;
pub use secrets::*;
