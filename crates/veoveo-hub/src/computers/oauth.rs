//! This gateway is an ordinary public PKCE client of the existing Hub issuer.
use anyhow::{Context, Result, ensure};
use axum::{
    extract::{Query, rejection::QueryRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{TimeDelta, Utc};
use secrecy::SecretString;
use serde::{Deserialize, Deserializer};
use sha2::{Digest, Sha256};
use veoveo_types::Sha256Digest;

use super::{ComputerRuntime, provisioning::LoginGrant};
use crate::{
    BearerToken, JwtAuthConfig, JwtVerifier,
    contract::{OAuthAuthorizationCode, PkceCodeVerifier},
    custody_client::CustodyClient,
    http::{auth_support::allowed_hub_jwt_algorithms, load_resource_authorization_jwks},
    problem::Problem,
};

pub(super) const SESSION_COOKIE: &str = "__Host-computer";
const LOGIN_COOKIE: &str = "__Host-computer-login";
const LOGIN_TTL: i64 = 600;
const TOKEN_BYTES: usize = 64 * 1024;

pub(super) fn opaque() -> Result<String> {
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random)
        .map_err(|_| anyhow::anyhow!("Computer random source unavailable"))?;
    Ok(hex::encode(random))
}
pub(super) fn digest(value: &str) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(value.as_bytes()).into())
}
fn valid_opaque(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
/// Refuse duplicated cookie names rather than choose one proxy/browser spelling.
pub(super) fn cookie(headers: &HeaderMap, name: &str) -> Result<Option<String>> {
    let mut found = None;
    for value in headers.get_all(header::COOKIE) {
        let value = value.to_str().context("invalid Computer cookie")?;
        for item in value.split(';') {
            if let Some((key, value)) = item.trim().split_once('=')
                && key == name
            {
                ensure!(
                    found.is_none() && valid_opaque(value),
                    "invalid or duplicate Computer cookie"
                );
                found = Some(value.to_owned());
            }
        }
    }
    Ok(found)
}
fn set_cookie(name: &str, value: &str, age: i64) -> Result<HeaderValue> {
    ensure!(
        valid_opaque(value) || value.is_empty(),
        "invalid Computer cookie value"
    );
    Ok(HeaderValue::from_str(&format!(
        "{name}={value}; Path=/; Max-Age={age}; Secure; HttpOnly; SameSite=Lax"
    ))?)
}
pub(super) fn private(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response
}
pub(super) fn redirect(location: &str) -> Result<Response> {
    let mut response = StatusCode::SEE_OTHER.into_response();
    response
        .headers_mut()
        .insert(header::LOCATION, HeaderValue::from_str(location)?);
    Ok(private(response))
}
pub(super) fn unavailable() -> Response {
    private(
        Problem::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "computer-unavailable",
            "Computer unavailable",
        )
        .detail("Sign in again if authorization or setup was interrupted.")
        .into_response(),
    )
}
pub(super) fn denied() -> Response {
    private(
        Problem::new(
            StatusCode::UNAUTHORIZED,
            "computer-authorization-required",
            "Computer authorization required",
        )
        .into_response(),
    )
}

/// The admitted browser session of the request's cookie, if any.
pub(super) async fn admitted(
    runtime: &ComputerRuntime,
    headers: &HeaderMap,
) -> Result<Option<(String, super::BrowserSession)>> {
    let Some(value) = cookie(headers, SESSION_COOKIE)? else {
        return Ok(None);
    };
    Ok(runtime
        .hub
        .computer_browser_session(
            &runtime.catalog.current(),
            &runtime.config,
            &digest(&value),
            None,
        )
        .await?
        .map(|session| (value, session)))
}

/// `GET /computer`: the way into the Computer. Signs in, creates a missing sandbox at
/// the current version, shows the start page while the sandbox is created or started,
/// and goes to the Computer once it is ready, or to the status page when a failure or a
/// sandbox the Hub can no longer serve needs the owner.
pub(super) async fn entry(runtime: ComputerRuntime, headers: HeaderMap) -> Response {
    let result = async {
        let Some((value, session)) = admitted(&runtime, &headers).await? else {
            return sign_in(&runtime).await;
        };
        let computer = session.computer();
        match runtime.enter(computer).await? {
            super::lifecycle::Entry::Ready => {
                // Authority may change during entry; do not publish a ready location
                // based on the earlier admission alone.
                ensure!(
                    runtime
                        .hub
                        .computer_browser_session(
                            &runtime.catalog.current(),
                            &runtime.config,
                            &digest(&value),
                            Some(&computer.id)
                        )
                        .await?
                        .is_some(),
                    "Computer admission ended during entry"
                );
                redirect(&computer.base_path())
            }
            super::lifecycle::Entry::Starting(Some(operation)) => {
                let changes =
                    super::version::whats_new(&computer.provisioning, &operation.provisioning).ok();
                runtime
                    .hub
                    .hub_store()
                    .show_computer_operation(computer, operation.id)
                    .await?;
                Ok(super::pages::starting(
                    operation.provisioning.info().ok().as_ref(),
                    changes.as_ref(),
                ))
            }
            super::lifecycle::Entry::Starting(None) => Ok(super::pages::starting(None, None)),
            super::lifecycle::Entry::Attention => redirect(super::pages::STATUS),
        }
    }
    .await;
    result.unwrap_or_else(|error| {
        // Never log a callback query, provider response, cookie or grant.
        tracing::warn!(error = %error, "Computer entry unavailable");
        refused(&error)
    })
}

/// The page of an entry that cannot continue: Computers unavailable and why, or the
/// generic unavailable answer.
pub(super) fn refused(error: &anyhow::Error) -> Response {
    match error.downcast_ref::<super::provisioning::Unavailable>() {
        Some(cause) => super::pages::unavailable(&cause.0),
        None => unavailable(),
    }
}

/// Ordinary PKCE sign-in at this Hub, with the current version's admitted defaults.
async fn sign_in(runtime: &ComputerRuntime) -> Result<Response> {
    let catalog = runtime.catalog.current();
    let current = runtime.installation.current().await?;
    let state = opaque()?;
    let browser = opaque()?;
    let verifier = PkceCodeVerifier::parse(opaque()?)?;
    runtime.hub.hub_store().prune_computer_logins().await?;
    runtime
        .hub
        .hub_store()
        .insert_computer_login(
            &digest(&state),
            &digest(&browser),
            &verifier,
            Utc::now() + TimeDelta::seconds(LOGIN_TTL),
            &current.spec,
        )
        .await?;
    let profile = catalog
        .profile(&runtime.config.profile)
        .context("Computer profile unavailable")?;
    let mut authorization = runtime.public.join("oauth/authorize")?;
    authorization
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", runtime.config.oauth_client.as_str())
        .append_pair(
            "redirect_uri",
            runtime.public.join("computer/callback")?.as_str(),
        )
        .append_pair(
            "code_challenge",
            &URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_str().as_bytes())),
        )
        .append_pair("code_challenge_method", "S256")
        .append_pair(
            "scope",
            &profile
                .required_scopes
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" "),
        )
        .append_pair("resource", profile.protected_resource.as_str())
        .append_pair("work_context", runtime.config.work_context.as_str())
        .append_pair("state", &state);
    let mut response = redirect(authorization.as_str())?;
    response.headers_mut().append(
        header::SET_COOKIE,
        set_cookie(LOGIN_COOKIE, &browser, LOGIN_TTL)?,
    );
    Ok(response)
}
#[derive(Deserialize)]
pub(super) struct Callback {
    state: String,
    code: Option<String>,
    error: Option<String>,
}
#[derive(Deserialize)]
struct Tokens {
    #[serde(deserialize_with = "secret")]
    access_token: SecretString,
    #[serde(deserialize_with = "secret")]
    refresh_token: SecretString,
    token_type: String,
}
fn secret<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<SecretString, D::Error> {
    String::deserialize(deserializer).map(SecretString::from)
}
pub(super) async fn callback(
    runtime: ComputerRuntime,
    headers: HeaderMap,
    callback: std::result::Result<Query<Callback>, QueryRejection>,
) -> Response {
    let Ok(Query(callback)) = callback else {
        return private(
            Problem::new(
                StatusCode::BAD_REQUEST,
                "computer-callback-invalid",
                "Invalid Computer callback",
            )
            .into_response(),
        );
    };
    match finish(runtime, headers, callback).await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(error = %error, "Computer callback refused or unavailable; reauthorization required");
            unavailable()
        }
    }
}
async fn finish(
    runtime: ComputerRuntime,
    headers: HeaderMap,
    callback: Callback,
) -> Result<Response> {
    ensure!(
        valid_opaque(&callback.state),
        "Computer callback state invalid"
    );
    let browser = cookie(&headers, LOGIN_COOKIE)?.context("Computer initiating browser missing")?;
    let login = runtime
        .hub
        .hub_store()
        .consume_computer_login(&digest(&callback.state), &digest(&browser))
        .await?
        .context("Computer login expired or consumed")?;
    let provisioner = runtime.installation.for_spec(login.provisioning)?;
    let mut runtime = runtime.clone();
    runtime.config = provisioner.config.clone();
    let verifier = login.verifier;
    ensure!(callback.error.is_none(), "Computer authorization refused");
    let code = OAuthAuthorizationCode::parse(callback.code.context("Computer code missing")?)?;
    let catalog = runtime.catalog.current();
    let profile = catalog
        .profile(&runtime.config.profile)
        .context("Computer profile unavailable")?;
    let server = catalog
        .authorization_server(&profile.authorization_server)
        .context("Computer issuer unavailable")?;
    let http = runtime.current_http();
    let callback_uri = runtime.public.join("computer/callback")?;
    let form = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.extend_pairs([
            ("grant_type", "authorization_code"),
            ("client_id", runtime.config.oauth_client.as_str()),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("redirect_uri", callback_uri.as_str()),
            ("resource", profile.protected_resource.as_str()),
        ]);
        form.finish()
    };
    let mut response = http
        .post(runtime.public.join("oauth/token")?)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("Computer code exchange unavailable"))?;
    ensure!(
        response.status().is_success(),
        "Computer code exchange refused"
    );
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= TOKEN_BYTES as u64),
        "Computer token response exceeds bound"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("Computer token read unavailable"))?
    {
        ensure!(
            bytes.len().saturating_add(chunk.len()) <= TOKEN_BYTES,
            "Computer token response exceeds bound"
        );
        bytes.extend_from_slice(&chunk);
    }
    let tokens: Tokens = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("Computer token response malformed"))?;
    use secrecy::ExposeSecret;
    ensure!(
        tokens.token_type.eq_ignore_ascii_case("bearer")
            && !tokens.refresh_token.expose_secret().is_empty(),
        "Computer token response grant invalid"
    );
    let bearer = BearerToken::from_authorization_header(&format!(
        "Bearer {}",
        tokens.access_token.expose_secret()
    ))
    .map_err(|_| anyhow::anyhow!("Computer access token malformed"))?;
    let jwks = load_resource_authorization_jwks(
        &catalog,
        server,
        runtime.public.as_str().trim_end_matches('/'),
        &http,
    )
    .await?;
    let config = JwtAuthConfig::new(
        server.issuer.clone(),
        profile.protected_resource.clone(),
        profile.required_scopes.iter().cloned().collect(),
        allowed_hub_jwt_algorithms(),
    )?;
    let verified = JwtVerifier::new(config, jwks)
        .with_extensions(runtime.hub.token_extension_registry()?)?
        .verify(&bearer)
        .map_err(|_| anyhow::anyhow!("Computer access token refused"))?;
    let subject = runtime
        .hub
        .resolve_authenticated_subject(&catalog, verified)
        .await?;
    ensure!(
        subject.access_token.oauth_client_id == runtime.config.oauth_client
            && subject.authority.work_context == runtime.config.work_context,
        "Computer user grant bound to another client/context"
    );
    ensure!(
        runtime
            .hub
            .access_token_session_valid(
                &profile.protected_resource,
                &server.id,
                &subject.access_token,
                &subject.principal
            )
            .await?,
        "Computer session revoked"
    );
    if let Some(id) = &subject.access_token.jwt_id {
        ensure!(
            runtime
                .hub
                .jwt_revocation(&profile.id, &subject.access_token.issuer, id, Utc::now())
                .await?
                .is_none(),
            "Computer access token revoked"
        );
    }
    let tenant = subject
        .principal
        .tenant
        .as_ref()
        .context("Computer user tenant missing")?;
    let computer = runtime
        .hub
        .hub_store()
        .ensure_computer(tenant, &subject.principal.id, &provisioner.spec)
        .await?;
    let family_id = subject
        .access_token
        .session_family
        .context("Computer grant has no native session family")?;
    let family = runtime
        .hub
        .hub_store()
        .refresh_family(&family_id)
        .await?
        .context("Computer family missing")?;
    let session = runtime
        .hub
        .admit_computer_family(&catalog, &runtime.config, computer.clone(), family)
        .await?
        .context("Computer grant authority denied")?;
    let cookie = opaque()?;
    let cookie_digest = digest(&cookie);
    // Each handoff gives the Computer's own Lore client a new secret.
    let lore = CustodyClient::new(computer.native_name())?;
    let handoff = runtime
        .hub
        .hub_store()
        .begin_computer_handoff(&computer, &lore.digest())
        .await?;
    let grant = LoginGrant {
        client: runtime.config.oauth_client.clone(),
        access: tokens.access_token,
        refresh: tokens.refresh_token,
        access_expiry: subject.access_token.expires_at,
        lore,
    };
    let worker_runtime = runtime.clone();
    // Detached ownership is deliberate: disconnect/cancellation cannot abandon a
    // native refresh handoff halfway through while allowing another owner worker.
    let worker = tokio::spawn(async move {
        tokio::time::timeout(std::time::Duration::from_secs(600), async {
            let claim = worker_runtime
                .hub
                .hub_store()
                .lock_computer_handoff(&computer, handoff)
                .await?
                .context("Computer handoff superseded")?;
            let current = worker_runtime.catalog.current();
            let family = worker_runtime
                .hub
                .hub_store()
                .refresh_family(&family_id)
                .await?
                .context("Computer family missing")?;
            worker_runtime
                .hub
                .admit_computer_family(
                    &current,
                    &worker_runtime.config,
                    claim.computer.clone(),
                    family,
                )
                .await?
                .context("Computer grant authority ended")?;
            worker_runtime
                .installation
                .for_computer(&claim.computer)?
                .handoff(&current, &claim.computer, grant)
                .await?;
            let family = worker_runtime
                .hub
                .hub_store()
                .refresh_family(&family_id)
                .await?
                .context("Computer family missing after custody")?;
            worker_runtime
                .hub
                .admit_computer_family(
                    &worker_runtime.catalog.current(),
                    &worker_runtime.config,
                    claim.computer.clone(),
                    family,
                )
                .await?
                .context("Computer grant authority ended")?;
            claim.complete(&family_id, &cookie_digest).await?;
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("Computer custody deadline")?
    });
    worker.await.context("Computer custody worker failed")??;
    // Entry creates the sandbox, on the start page, when it is missing.
    let mut response = redirect(super::pages::ENTRY)?;
    response.headers_mut().append(
        header::SET_COOKIE,
        set_cookie(
            SESSION_COOKIE,
            &cookie,
            (session.expires_at() - Utc::now()).num_seconds().max(0),
        )?,
    );
    response
        .headers_mut()
        .append(header::SET_COOKIE, set_cookie(LOGIN_COOKIE, "", 0)?);
    Ok(response)
}

/// Parameterless same-origin logout. Family/owner come only from fresh native
/// BrowserSession admission; no caller-supplied identifier or exported grant.
pub(super) async fn logout(runtime: ComputerRuntime, headers: HeaderMap) -> Response {
    if !super::proxy::same_origin(&runtime, &headers) {
        return private(
            Problem::new(
                StatusCode::FORBIDDEN,
                "computer-origin-refused",
                "Computer origin refused",
            )
            .into_response(),
        );
    }
    let started = std::time::Instant::now();
    let result = async {
        let Some(value) = cookie(&headers, SESSION_COOKIE)? else {
            return Ok::<_, anyhow::Error>(());
        };
        let Some(session) = runtime
            .hub
            .computer_browser_session(
                &runtime.catalog.current(),
                &runtime.config,
                &digest(&value),
                None,
            )
            .await?
        else {
            return Ok(());
        };
        use crate::{
            audit::{
                Authentication, AuthenticationActivity, authentication_reason, hub_audit,
                profile_target,
            },
            contract::{AuthMethod, AuthReasonCode},
        };
        use veoveo_audit_contract::{AuditActor, AuditDraft, AuditOutcome, AuditPrincipalKind};
        let principal = session.principal();
        let detail = hub_audit().detail(&Authentication {
            activity: AuthenticationActivity::Revoke,
            method: AuthMethod::BrowserSession,
            duration_ms: u64::try_from(started.elapsed().as_millis())?,
            session_family: Some(session.family().clone()),
            client: Some(runtime.config.oauth_client.clone()),
            scopes: principal.scopes.clone(),
            data_labels: principal.data_labels.clone(),
        })?;
        let draft = AuditDraft::builder(
            crate::http::auth_support::audit_request(),
            detail,
            AuditOutcome::Success,
        )
        .actor(AuditActor {
            principal: principal.id.clone(),
            kind: AuditPrincipalKind::User,
            tenant: principal.tenant.clone(),
            work_context: Some(runtime.config.work_context.clone()),
        })
        .target(profile_target(&runtime.config.profile)?)
        .reason(
            authentication_reason(AuthReasonCode::RefreshTokenRevoked)
                .context("native revocation reason missing")?,
        )
        .build()?;
        let audit = runtime.hub.audit_write(&draft)?;
        runtime
            .hub
            .hub_store()
            .revoke_computer_session(session.computer(), session.family(), &audit)
            .await?;
        Ok(())
    }
    .await;
    if result.is_err() {
        return unavailable();
    }
    let mut response = private(StatusCode::NO_CONTENT.into_response());
    response.headers_mut().append(
        header::SET_COOKIE,
        set_cookie(SESSION_COOKIE, "", 0).expect("fixed cookie"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_binding_refuses_duplicate_or_malformed_cookies() {
        let token = "a".repeat(64);
        let mut headers = HeaderMap::new();
        headers.append(
            header::COOKIE,
            HeaderValue::from_str(&format!("{SESSION_COOKIE}={token}")).unwrap(),
        );
        assert_eq!(
            cookie(&headers, SESSION_COOKIE).unwrap(),
            Some(token.clone())
        );
        headers.append(
            header::COOKIE,
            HeaderValue::from_str(&format!("{SESSION_COOKIE}={token}")).unwrap(),
        );
        assert!(cookie(&headers, SESSION_COOKIE).is_err());
        assert!(set_cookie(SESSION_COOKIE, "injected; value", 10).is_err());
        let value = set_cookie(SESSION_COOKIE, &token, 10).unwrap();
        assert!(
            value
                .to_str()
                .unwrap()
                .contains("Secure; HttpOnly; SameSite=Lax")
        );
    }
}
