//! HTTP binding of the registration protocol: RFC 8414 metadata, the issuer's JWK Set,
//! the RFC 7591 registration endpoint and the RFC 7592 client configuration endpoint.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Serialize;

use super::{RegistrationService, service::Bearer, service::JWKS_PATH};

/// RFC 7517 §8.5: the media type of a JWK Set.
pub const JWK_SET_MEDIA_TYPE: &str = "application/jwk-set+json";

/// RFC 8414 metadata of the Gateway's own issuer. Registered servers read
/// `registration_endpoint` here and exchange tokens to call each other at
/// `token_endpoint`; their forwarded tokens carry this `issuer` and verify with the keys
/// at `jwks_uri`.
#[derive(Serialize)]
struct IssuerMetadata {
    issuer: String,
    jwks_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    registration_endpoint: Option<String>,
    token_endpoint: String,
    /// Servers use no authorization endpoint, and only the RFC 8693 grant.
    response_types_supported: [&'static str; 0],
    grant_types_supported: [&'static str; 1],
    token_endpoint_auth_methods_supported: [&'static str; 1],
    token_endpoint_auth_signing_alg_values_supported: [&'static str; 1],
}

pub fn router(service: RegistrationService) -> Router {
    Router::new()
        .route("/.well-known/oauth-authorization-server", get(metadata))
        .route(JWKS_PATH, get(jwks))
        .route("/oauth/register", post(register))
        .route(
            "/oauth/register/{client_id}",
            get(read).put(update).delete(delete),
        )
        .with_state(service)
}

async fn metadata(State(service): State<RegistrationService>) -> Json<IssuerMetadata> {
    Json(IssuerMetadata {
        issuer: service.issuer().as_str().to_owned(),
        jwks_uri: service.jwks_uri().to_string(),
        registration_endpoint: service
            .enabled()
            .then(|| service.registration_endpoint().to_string()),
        token_endpoint: service.token_endpoint().to_string(),
        response_types_supported: [],
        grant_types_supported: [veoveo_sdk_rust::calls::TOKEN_EXCHANGE_GRANT_TYPE],
        token_endpoint_auth_methods_supported: ["private_key_jwt"],
        token_endpoint_auth_signing_alg_values_supported: ["EdDSA"],
    })
}

/// The keys that verify forwarded access tokens. A server that meets an unknown `kid`
/// reads them again, so no cache may answer for the Gateway.
async fn jwks(State(service): State<RegistrationService>) -> Response {
    (
        [
            (header::CONTENT_TYPE, JWK_SET_MEDIA_TYPE),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        Json(service.access_token_keys()),
    )
        .into_response()
}

fn bearer(headers: &HeaderMap) -> Bearer<'_> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, token)| scheme.eq_ignore_ascii_case("bearer") && !token.is_empty())
        .map_or(Bearer::Missing, |(_, token)| {
            Bearer::Presented(token.trim())
        })
}

fn no_store(status: StatusCode, body: impl Serialize) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
}

async fn register(
    State(service): State<RegistrationService>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    match service.register(bearer(&headers), &body).await {
        Ok(information) => no_store(StatusCode::CREATED, information),
        Err(error) => error.into_response(),
    }
}

async fn read(
    State(service): State<RegistrationService>,
    Path(client_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    match service.read(&client_id, bearer(&headers)).await {
        Ok(information) => no_store(StatusCode::OK, information),
        Err(error) => error.into_response(),
    }
}

async fn update(
    State(service): State<RegistrationService>,
    Path(client_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    match service.update(&client_id, bearer(&headers), &body).await {
        Ok(information) => no_store(StatusCode::OK, information),
        Err(error) => error.into_response(),
    }
}

async fn delete(
    State(service): State<RegistrationService>,
    Path(client_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    match service.delete(&client_id, bearer(&headers)).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error.into_response(),
    }
}
