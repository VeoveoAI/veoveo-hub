use crate::{
    runtime::{AppState, current_catalog, public_authorization_server},
    tokens::authorization_server_jwks_from_signing_key,
};
use axum::{
    Json,
    extract::{Path as AxumPath, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE},
    },
    response::IntoResponse,
};
use veoveo_mcp_contract::GatewayProfileId;
pub(super) async fn protected_resource_metadata(
    State(state): State<AppState>,
    AxumPath(profile): AxumPath<String>,
) -> impl IntoResponse {
    let Ok(profile_id) = GatewayProfileId::parse(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let catalog = current_catalog(&state.catalog);
    match catalog.protected_resource_metadata(&profile_id) {
        Ok(metadata) => {
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            (StatusCode::OK, headers, Json(metadata)).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) async fn authorization_server_metadata(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let catalog = current_catalog(&state.catalog);
    let Some(authorization_server) = public_authorization_server(&catalog, &state.public_base_url)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match catalog.authorization_server_metadata_for_server(&authorization_server.id) {
        Ok(mut metadata) => {
            metadata.jwks_uri = Some(format!(
                "{}/oauth/jwks.json",
                state.public_base_url.trim_end_matches('/')
            ));
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            (StatusCode::OK, headers, Json(metadata)).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) async fn authorization_server_jwks(
    State(state): State<AppState>,
) -> axum::response::Response {
    let catalog = current_catalog(&state.catalog);
    let Some(authorization_server) = public_authorization_server(&catalog, &state.public_base_url)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let jwks =
        match authorization_server_jwks_from_signing_key(&catalog, authorization_server).await {
            Ok(jwks) => jwks,
            Err(err) => {
                tracing::error!("failed to build authorization server JWKS: {err}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300, must-revalidate"),
    );
    (StatusCode::OK, headers, Json(jwks)).into_response()
}

pub(super) use veoveo_service_gateway::http::authenticate_profile as authenticate_mcp;

#[cfg(test)]
mod tests {
    use veoveo_service_gateway::http::auth_support::AccessTokenSigningKey;

    #[test]
    fn access_token_jwks_contains_only_the_public_ed25519_key() {
        let key = AccessTokenSigningKey::from_base64(
            "MC4CAQAwBQYDK2VwBCIEII4AsVspz8h7mpqvOkgslJP07HfqpiWMZA+6Ii90lVBl",
        )
        .unwrap();
        let jwks = serde_json::to_value(key.jwks().unwrap()).unwrap();
        let public = &jwks["keys"][0];
        assert_eq!(public["kty"], "OKP");
        assert_eq!(public["crv"], "Ed25519");
        assert_eq!(public["alg"], "EdDSA");
        assert_eq!(public["kid"], key.key_id().as_str());
        assert!(public.get("d").is_none());
        let header = key.header();
        assert_eq!(header.kid.as_deref(), Some(key.key_id().as_str()));
        assert_eq!(header.typ.as_deref(), Some("at+jwt"));
        assert_eq!(header.alg, jsonwebtoken::Algorithm::EdDSA);
    }
}
