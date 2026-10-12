//! Server registration: the RFC 7591 and RFC 7592 endpoints, namespace claims,
//! leases, migrations through the core module runner, and the snapshot of registered
//! servers each catalog generation routes to.

mod endpoint;
mod modules;
mod service;
mod snapshot;
mod store;
mod watch;

use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use veoveo_mcp_ext_registration::{RegistrationErrorCode, RegistrationErrorResponse};

pub use endpoint::router;
pub use modules::{apply_modules, module_registry};
pub use service::{RegistrationIdentity, RegistrationService, ServerDatabase};
pub use snapshot::{RegisteredServer, RegistrationSnapshot, ServerCapabilities};

/// Loads every module a namespace claim recorded, so startup migrations know the
/// complete history the database holds.
pub async fn claimed_modules(
    store: &veoveo_platform_store::PlatformStore,
) -> anyhow::Result<Vec<veoveo_modules::ModuleManifest>> {
    Ok(store::RegistrationStore::new(store.clone())
        .claims()
        .await?
        .into_iter()
        .filter_map(|claim| claim.module)
        .collect())
}

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
