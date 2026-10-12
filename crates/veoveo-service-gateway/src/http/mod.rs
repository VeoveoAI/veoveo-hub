//! Shared HTTP admission for profile-authenticated routes.
mod context;
pub use context::*;

pub mod auth_support;
mod authentication;
pub use authentication::{
    authenticate_profile, load_resource_authorization_jwks, profile_from_request,
    profile_from_route,
};

pub mod action_audit;
