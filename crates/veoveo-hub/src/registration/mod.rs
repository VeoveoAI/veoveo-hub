//! Server registration: the RFC 7591 and RFC 7592 endpoints, namespace claims,
//! instances and their registration tokens, the PostgreSQL login each registration
//! receives, the removal of registrations whose processes ended, and the snapshot of
//! registered servers each catalog generation routes to.

mod endpoint;
mod service;
mod snapshot;
mod store;
mod watch;

use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use veoveo_hub_sdk::registration::{RegistrationErrorCode, RegistrationErrorResponse};

pub use endpoint::{JWK_SET_MEDIA_TYPE, router};
pub use service::{
    NewInstance, RegistrationIdentity, RegistrationService, ServerDatabase, create_instance,
    remove_instance,
};
pub use snapshot::{RegisteredServer, RegistrationSnapshot, ServerCapabilities};
pub use watch::HOUSEKEEPING_INTERVAL;

/// Why a registration request fails, in the error format of the RFC that defines the
/// endpoint.
#[derive(Debug)]
pub enum RegistrationError {
    /// RFC 6750 §3: a missing or wrong initial or registration access token.
    Unauthorized {
        token_sent: bool,
    },
    /// RFC 7591 §3.2.2.
    Rejected {
        code: RegistrationErrorCode,
        description: String,
    },
    Internal(anyhow::Error),
}

impl RegistrationError {
    pub fn rejected(code: RegistrationErrorCode, description: impl Into<String>) -> Self {
        Self::Rejected {
            code,
            description: description.into(),
        }
    }
}

impl From<anyhow::Error> for RegistrationError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

impl IntoResponse for RegistrationError {
    fn into_response(self) -> Response {
        match self {
            Self::Unauthorized { token_sent } => (
                StatusCode::UNAUTHORIZED,
                [(
                    header::WWW_AUTHENTICATE,
                    if token_sent {
                        r#"Bearer error="invalid_token""#
                    } else {
                        "Bearer"
                    },
                )],
            )
                .into_response(),
            Self::Rejected { code, description } => (
                StatusCode::BAD_REQUEST,
                [(header::CACHE_CONTROL, "no-store")],
                Json(RegistrationErrorResponse {
                    error: code,
                    error_description: Some(description),
                }),
            )
                .into_response(),
            Self::Internal(error) => {
                tracing::error!("server registration failed: {error:#}");
                crate::problem::Problem::internal().into_response()
            }
        }
    }
}
