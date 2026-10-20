//! RFC 8628 public-client authorization, using the existing enterprise browser
//! broker for the human proof. Device and browser capabilities are stored as hashes.
use crate::{
    audit::{AuthAuditRecord, internal_error_response, record_protocol_auth_audit},
    http_util::{
        TokenResponse, oauth_error_response, random_oauth_state, scope_string, token_response,
    },
    oauth_grants::{TokenRequest, requested_token_scopes},
    runtime::{AppState, current_catalog},
};
use anyhow::{Context, Result, ensure};
use axum::{
    Form, Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use chrono::{TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_hub::{
    HubRefreshIssueRequest, RegisteredProtectedResource,
    contract::{AuthOutcome, AuthReasonCode, OAuthClientAuthMethod, OAuthGrantType},
    control_plane::{AuthorizationDestination, OAuthClientRegistration},
    store::{DevicePoll, NewDeviceAuthorization},
    token_profile::{ProfileRequest, TokenContext},
};
use veoveo_hub_sdk::OAuthClientId;
use veoveo_types::Sha256Digest;

pub(super) const GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const BROWSER_COOKIE: &str = "__Host-hub-device";
const TTL: i64 = 600;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthorizationRequest {
    client_id: String,
    scope: String,
}
#[derive(Serialize)]
struct AuthorizationResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: String,
    expires_in: u64,
    interval: u64,
}

pub(super) fn public_client(
    resource: RegisteredProtectedResource<'_>,
    client: &OAuthClientRegistration,
    grant: OAuthGrantType,
) -> bool {
    resource.supports(grant)
        && client.grant_types.contains(&grant)
        && client.auth_methods == std::collections::BTreeSet::from([OAuthClientAuthMethod::None])
        && &client.authorization_server == resource.authorization_server()
        && client
            .allowed_resources
            .contains(resource.protected_resource())
}
pub(super) fn digest(value: &str) -> Sha256Digest {
    Sha256Digest::parse(format!(
        "sha256:{}",
        hex::encode(Sha256::digest(value.as_bytes()))
    ))
    .expect("SHA-256 encoding")
}
fn cookie(headers: &HeaderMap) -> Result<String> {
    let values = headers
        .get_all(header::COOKIE)
        .iter()
        .map(|v| v.to_str())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut matches = values
        .iter()
        .flat_map(|v| v.split(';'))
        .filter_map(|v| v.trim().split_once('='))
        .filter(|(name, _)| *name == BROWSER_COOKIE)
        .map(|(_, value)| value);
    let value = matches.next().context("initiating browser missing")?;
    ensure!(
        matches.next().is_none()
            && value.len() == 64
            && value.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid initiating browser"
    );
    Ok(value.to_owned())
}
pub(super) fn browser_digest(headers: &HeaderMap) -> Result<Sha256Digest> {
    Ok(digest(&cookie(headers)?))
}
fn user_code(value: &str) -> Option<String> {
    let compact = value.replace('-', "").to_ascii_uppercase();
    (compact.len() == 16 && compact.bytes().all(|b| b.is_ascii_hexdigit())).then_some(compact)
}
fn no_store(response: impl IntoResponse) -> Response {
    let mut response = response.into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response
}
pub(crate) async fn authorize(
    State(state): State<AppState>,
    Form(request): Form<AuthorizationRequest>,
) -> Response {
    let catalog = current_catalog(&state.catalog);
    let resource = match super::resolve_oauth_resource(&catalog, &request.client_id, None) {
        Ok(r) => r,
        Err(e) => return *e,
    };
    let Ok(id) = OAuthClientId::parse(&request.client_id) else {
        return refusal("invalid_client");
    };
    let Some(client) = catalog.oauth_client(&id).filter(|c| {
        public_client(resource, c, OAuthGrantType::DeviceAuthorization)
            && c.grant_types.contains(&OAuthGrantType::RefreshToken)
    }) else {
        return refusal("invalid_client");
    };
    let scopes = match requested_token_scopes(&catalog, resource, client, Some(&request.scope)) {
        Ok(s) if s == resource.supported_scopes(&catalog) => s,
        _ => return refusal("invalid_scope"),
    };
    if resource.oidc_client(&catalog).is_none() {
        return unavailable();
    }
    let device = match random_oauth_state() {
        Ok(v) => v.to_string(),
        Err(e) => return internal_error_response(e),
    };
    let code = uuid::Uuid::new_v4().simple().to_string()[..16].to_ascii_uppercase();
    let now = Utc::now();
    if let Err(error) = state
        .hub_state
        .hub_store()
        .create_device_authorization(NewDeviceAuthorization {
            id: uuid::Uuid::now_v7(),
            device: &digest(&device),
            user: &digest(&code),
            resource: resource.protected_resource(),
            client: &id,
            work_context: &client.default_work_context,
            scopes: &scopes,
            now,
            expires_at: now + TimeDelta::seconds(TTL),
        })
        .await
    {
        return internal_error_response(error);
    }
    let verification_uri = format!(
        "{}/oauth/device",
        state.public_base_url.trim_end_matches('/')
    );
    no_store(Json(AuthorizationResponse {
        device_code: device,
        user_code: format!(
            "{}-{}-{}-{}",
            &code[..4],
            &code[4..8],
            &code[8..12],
            &code[12..]
        ),
        verification_uri_complete: format!("{verification_uri}?user_code={code}"),
        verification_uri,
        expires_in: TTL as u64,
        interval: 5,
    }))
}
#[derive(Deserialize)]
pub(crate) struct VerificationQuery {
    user_code: Option<String>,
}
fn escaped(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
pub(crate) async fn verification(
    State(state): State<AppState>,
    Query(query): Query<VerificationQuery>,
) -> Response {
    let browser = match random_oauth_state() {
        Ok(v) => v.to_string(),
        Err(e) => return internal_error_response(e),
    };
    let code = query
        .user_code
        .as_deref()
        .and_then(user_code)
        .unwrap_or_default();
    let mut label = "Enter the code shown by your client".to_owned();
    let mut form_action = "'self'".to_owned();
    if !code.is_empty() {
        match state
            .hub_state
            .hub_store()
            .device_authorization(&digest(&code), Utc::now())
            .await
        {
            Ok(Some(request)) => {
                let catalog = current_catalog(&state.catalog);
                let Some(resource) = catalog.registered_protected_resource(&request.resource)
                else {
                    return refusal("invalid_target");
                };
                if !catalog.oauth_client(&request.client).is_some_and(|client| {
                    public_client(resource, client, OAuthGrantType::DeviceAuthorization)
                        && request.scopes.is_subset(&client.allowed_scopes)
                        && request.scopes == resource.supported_scopes(&catalog)
                }) {
                    return refusal("invalid_client");
                }
                let Some(endpoint) = resource
                    .oidc_client(&catalog)
                    .and_then(|client| catalog.identity_provider(&client.identity_provider))
                    .and_then(|provider| provider.authorization_endpoint.as_ref())
                else {
                    return unavailable();
                };
                let origin = match url::Url::parse(endpoint.as_str()) {
                    Ok(endpoint) => endpoint.origin().ascii_serialization(),
                    Err(error) => return internal_error_response(error),
                };
                // Chrome applies form-action to the POST's broker redirect too.
                // Admit only this selected resource's trusted authorization origin.
                form_action.push(' ');
                form_action.push_str(&origin);
                label = format!(
                    "Approve client {} for {} only if this code matches your client",
                    request.client, request.resource
                );
            }
            Ok(None) => return refusal("expired_token"),
            Err(e) => return internal_error_response(e),
        }
    }
    let form = if code.is_empty() {
        // Select a code first so the approval page can bind its browser policy to
        // one resource/broker. Never put the browser capability in a GET query.
        "<form method=get><label>Code <input required name=user_code></label><button>Continue</button></form>".to_owned()
    } else {
        format!(
            "<form method=post><label>Code <input required name=user_code value=\"{}\"></label><input type=hidden name=browser value=\"{}\"><button name=decision value=approve>Continue to sign in</button><button name=decision value=deny>Deny</button></form>",
            escaped(&code),
            browser
        )
    };
    let html = format!(
        "<!doctype html><meta charset=utf-8><title>Device authorization</title><h1>Device authorization</h1><p>{}</p>{form}",
        escaped(&label)
    );
    let mut response = no_store(Html(html));
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&format!(
            "default-src 'none'; form-action {form_action}; frame-ancestors 'none'; base-uri 'none'"
        ))
        .expect("trusted authorization origin"),
    );
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&format!(
            "{BROWSER_COOKIE}={browser}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age={TTL}"
        ))
        .expect("fixed cookie"),
    );
    response
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Approval {
    user_code: String,
    browser: String,
    decision: String,
}
pub(crate) async fn approve(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<Approval>,
) -> Response {
    let Ok(expected) = url::Url::parse(&state.public_base_url) else {
        return unavailable();
    };
    if headers.get(header::ORIGIN).and_then(|h| h.to_str().ok())
        != Some(expected.origin().ascii_serialization().as_str())
        || cookie(&headers).ok().as_deref() != Some(form.browser.as_str())
    {
        return refusal("invalid_request");
    }
    let Some(code) = user_code(&form.user_code) else {
        return refusal("invalid_request");
    };
    let store = state.hub_state.hub_store();
    let request = match store.device_authorization(&digest(&code), Utc::now()).await {
        Ok(Some(r)) => r,
        Ok(None) => return refusal("expired_token"),
        Err(e) => return internal_error_response(e),
    };
    if form.decision == "deny" {
        return match store.deny_device_authorization(request.id).await {
            Ok(()) => no_store(Html("Device authorization denied.")),
            Err(e) => internal_error_response(e),
        };
    }
    if form.decision != "approve" {
        return refusal("invalid_request");
    }
    let catalog = current_catalog(&state.catalog);
    let Some(resource) = catalog.registered_protected_resource(&request.resource) else {
        return refusal("invalid_target");
    };
    let Some(client) = catalog.oauth_client(&request.client).filter(|c| {
        public_client(resource, c, OAuthGrantType::DeviceAuthorization)
            && request.scopes.is_subset(&c.allowed_scopes)
            && request.scopes == resource.supported_scopes(&catalog)
    }) else {
        return refusal("invalid_client");
    };
    let _ = client;
    let browser = digest(&form.browser);
    match store
        .begin_device_authorization(request.id, &digest(&code), &browser, Utc::now())
        .await
    {
        Ok(Some(_)) => {
            super::broker::begin(
                &state,
                &catalog,
                resource,
                request.client,
                request.work_context,
                request.scopes,
                AuthorizationDestination::Device {
                    request: request.id,
                    browser,
                },
            )
            .await
        }
        Ok(None) => refusal("invalid_request"),
        Err(e) => internal_error_response(e),
    }
}
pub(super) async fn token(
    state: &AppState,
    catalog: &veoveo_hub::HubCatalog,
    resource: RegisteredProtectedResource<'_>,
    server: &veoveo_hub::control_plane::ResourceAuthorizationServer,
    request: TokenRequest,
) -> Response {
    let started_at = std::time::Instant::now();
    let Ok(client_id) = OAuthClientId::parse(&request.client_id) else {
        return refusal("invalid_client");
    };
    let Some(client) = catalog
        .oauth_client(&client_id)
        .filter(|c| public_client(resource, c, OAuthGrantType::DeviceAuthorization))
    else {
        return refusal("invalid_client");
    };
    if request.client_assertion.is_some()
        || request.client_assertion_type.is_some()
        || request.client_secret.is_some()
        || request.scope.is_some()
    {
        return refusal("invalid_request");
    }
    let Some(code) = request.device_code.filter(|c| c.len() <= 512) else {
        return refusal("invalid_grant");
    };
    let poll = match state
        .hub_state
        .hub_store()
        .poll_device_authorization(&digest(&code), &client_id, Utc::now())
        .await
    {
        Ok(p) => p,
        Err(e) => return internal_error_response(e),
    };
    let (approved, principal, display) = match poll {
        DevicePoll::Pending => return refusal("authorization_pending"),
        DevicePoll::SlowDown => return refusal("slow_down"),
        DevicePoll::Denied => return refusal("access_denied"),
        DevicePoll::Expired => return refusal("expired_token"),
        DevicePoll::Invalid => return refusal("invalid_grant"),
        DevicePoll::Approved(approval) => {
            let approval = *approval;
            (approval.request, approval.principal, approval.display_name)
        }
    };
    if &approved.resource != resource.protected_resource()
        || approved.scopes != resource.supported_scopes(catalog)
        || !approved.scopes.is_subset(&client.allowed_scopes)
        || !public_client(resource, client, OAuthGrantType::RefreshToken)
    {
        return refusal("invalid_grant");
    }
    let refresh = match state
        .hub_state
        .issue_refresh_token(HubRefreshIssueRequest {
            authorization_server: &server.id,
            protected_resource: &approved.resource,
            oauth_client_id: &client_id,
            work_context: &approved.work_context,
            principal: &principal,
            principal_display_name: &display,
            scopes: &approved.scopes,
            now: Utc::now(),
        })
        .await
    {
        Ok(v) => v,
        Err(e) => return internal_error_response(e),
    };
    let token = match async {
        let context = TokenContext::admit(
            catalog,
            &approved.resource,
            &client_id,
            &principal,
            &approved.work_context,
            &approved.scopes,
            Some(&refresh.grant.family_id),
        )?
        .expires_before(refresh.grant.expires_at);
        state
            .token_profiles
            .issue(context, ProfileRequest::Login, Some(&display))
            .await
    }
    .await
    {
        Ok(t) => t,
        Err(error) if error.is_denied() => return refusal("access_denied"),
        Err(error) => {
            tracing::error!(%error, "device token issuance unavailable");
            return unavailable();
        }
    };
    if let Err(e) = record_protocol_auth_audit(
        &state.hub_state,
        resource,
        AuthAuditRecord {
            authorization_server: Some(server),
            client_id: Some(&client_id),
            principal: Some(&principal),
            outcome: AuthOutcome::Allow,
            reason: AuthReasonCode::AuthAllow,
            started_at,
        },
        veoveo_hub::contract::AuthMethod::DeviceAuthorization,
    )
    .await
    {
        return internal_error_response(e);
    }
    token_response(TokenResponse {
        expires_in: token.expires_in(),
        access_token: token.access_token,
        token_type: "Bearer",
        scope: scope_string(&token.scopes),
        refresh_token: Some(refresh.token.as_str().to_owned()),
        refresh_token_expires_in: Some(
            refresh
                .grant
                .expires_at
                .signed_duration_since(Utc::now())
                .num_seconds()
                .max(0) as u64,
        ),
    })
}
fn refusal(error: &'static str) -> Response {
    oauth_error_response(
        StatusCode::BAD_REQUEST,
        error,
        "device authorization could not be completed",
    )
}
fn unavailable() -> Response {
    oauth_error_response(
        StatusCode::SERVICE_UNAVAILABLE,
        "server_error",
        "authorization service is unavailable",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initiating_browser_binding_rejects_duplicate_cookies_and_user_code_is_canonical() {
        let mut headers = HeaderMap::new();
        let browser = "a".repeat(64);
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("{BROWSER_COOKIE}={browser}")).unwrap(),
        );
        assert_eq!(browser_digest(&headers).unwrap(), digest(&browser));
        headers.append(
            header::COOKIE,
            HeaderValue::from_str(&format!("{BROWSER_COOKIE}={browser}")).unwrap(),
        );
        assert!(browser_digest(&headers).is_err());
        assert_eq!(
            user_code("abcd-0123-4567-89ef").unwrap(),
            "ABCD0123456789EF"
        );
        assert!(user_code("<not-a-native-code>").is_none());
        assert_eq!(
            escaped("<client & \"resource\">"),
            "&lt;client &amp; &quot;resource&quot;&gt;"
        );
    }
}
