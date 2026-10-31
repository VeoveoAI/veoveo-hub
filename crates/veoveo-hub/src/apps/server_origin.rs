//! A server's own app origin, `{slug}.{apps domain}`, for a full web application. The
//! Apps listener tells it apart by `X-Forwarded-Host`, which the installation's tunnel sets
//! from the name the browser asked for. A person signs in through the issuer, which hands
//! the browser a single-use code bound to state the app origin set in it; the code opens a
//! session in a host-only cookie of the app origin. Each request then reaches the server's
//! authenticated routes on the instance the person selected, with a forwarded token, after
//! the session's family is admitted again and the server's `app_origin` requirement allows
//! the person.
use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use chrono::{TimeDelta, Utc};
use sha2::{Digest, Sha256};
use tower::ServiceExt as _;
use veoveo_hub_sdk::HubInternalTokenIssuer;
use veoveo_types::{ServerSlug, Sha256Digest};

use super::{bridge, origins::AppOrigins, pages};
use crate::{
    AuthenticatedSubject, HubCatalogHandle, HubState, HubUpstreamHttpClientPool,
    contract::HubAction,
    control_plane::{PolicyEffect, PolicyTarget, TraceId},
    http::ProfileAuthState,
    problem::Problem,
};

/// The session cookie on an app origin.
pub const SESSION_COOKIE: &str = "__Host-veoveo-app";
/// The state an app origin sets in the browser before it sends it to the issuer.
pub const STATE_COOKIE: &str = "__Host-veoveo-app-state";
/// The one path on every app origin that the Hub answers itself: a code's redemption.
pub const SESSION_PATH: &str = "/_veoveo/session";
/// Where on the issuer a person enters a server's app origin.
pub const ENTRY_PATH: &str = "/app-origins";
const CODE_SECONDS: i64 = 60;
const STATE_SECONDS: i64 = 600;
/// A forwarded token and the access token behind it live one request.
const BEARER_SECONDS: u32 = 60;
const MAX_RETURN_PATH: usize = 2048;

/// A random 256-bit value in hex, the shape of every Hub browser cookie value.
fn opaque() -> anyhow::Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| anyhow::anyhow!("random source: {error}"))?;
    Ok(hex::encode(bytes))
}

fn digest(value: &str) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(value.as_bytes()).into())
}

#[cfg(test)]
pub(super) fn tests_digest(value: &str) -> String {
    digest(value).as_str().to_owned()
}

fn opaque_value(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The one value of the request's cookie `name`; a duplicated or malformed one is none.
fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let mut found = None;
    for value in headers.get_all(header::COOKIE) {
        for item in value.to_str().ok()?.split(';') {
            if let Some((key, value)) = item.trim().split_once('=')
                && key == name
            {
                if found.is_some() || !opaque_value(value) {
                    return None;
                }
                found = Some(value.to_owned());
            }
        }
    }
    found
}

fn set_cookie(name: &str, value: &str, age: i64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{name}={value}; Path=/; Max-Age={age}; Secure; HttpOnly; SameSite=Lax"
    ))
    .expect("a cookie of hex and fixed attributes")
}

fn private(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

fn see_other(location: &str) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    match HeaderValue::from_str(location) {
        Ok(location) => {
            response.headers_mut().insert(header::LOCATION, location);
            private(response)
        }
        Err(_) => Problem::internal().into_response(),
    }
}

/// A path on an app origin to return to: absolute, at most 2 KiB, with no scheme-relative
/// start, backslash or control character, so it never names another origin.
pub fn return_path(value: &str) -> bool {
    value.starts_with('/')
        && !value.starts_with("//")
        && value.len() <= MAX_RETURN_PATH
        && !value.contains('\\')
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

/// The issuer's entry for `server`'s app origin, returning to `path` there for the browser
/// holding `state`.
fn entry_url(issuer: &str, server: &ServerSlug, path: &str, state: &Sha256Digest) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("path", path)
        .append_pair("state", state.as_str())
        .finish();
    format!("{issuer}{ENTRY_PATH}/{server}?{query}")
}

/// An entry the sign-in may return to: `/app-origins/{slug}?path=…&state=…` with a server
/// slug, a return path and a state digest, re-spelled canonically.
pub fn entry_from_path(value: &str) -> Option<String> {
    let url = url::Url::parse(&format!("https://entry.invalid{value}")).ok()?;
    let server = url.path().strip_prefix(&format!("{ENTRY_PATH}/"))?;
    let server = ServerSlug::parse(server).ok()?;
    let (path, state) = entry_query(url.query()?)?;
    Some(entry_url("", &server, &path, &state))
}

/// The return path and state of an entry's query, each exactly once.
fn entry_query(query: &str) -> Option<(String, Sha256Digest)> {
    let mut path = None;
    let mut state = None;
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        let slot = match key.as_ref() {
            "path" => &mut path,
            "state" => &mut state,
            _ => return None,
        };
        if slot.replace(value.into_owned()).is_some() {
            return None;
        }
    }
    let path = path.filter(|path| return_path(path))?;
    let state = Sha256Digest::parse(state?).ok()?;
    Some((path, state))
}

/// The page of a refusal on an app origin or its entry.
fn refused(status: StatusCode, title: &str, text: &str) -> Response {
    private(pages::message(status, title, text))
}

fn unavailable(error: &dyn std::fmt::Display) -> Response {
    tracing::warn!(%error, "app origin unavailable");
    refused(
        StatusCode::SERVICE_UNAVAILABLE,
        "App unavailable",
        "This app cannot be opened now. Try again in a moment.",
    )
}

/// Whether any registration of `server` declares an app origin.
fn declares_origin(catalog: &crate::HubCatalog, server: &ServerSlug) -> bool {
    catalog.registrations().iter().any(|registered| {
        &registered.slug == server
            && registered
                .surfaces
                .as_ref()
                .is_some_and(|surfaces| surfaces.app_origin)
    })
}

fn trace_id() -> TraceId {
    TraceId::parse(uuid::Uuid::now_v7().to_string()).expect("a UUID is a trace")
}

/// The app origins of every server that declares one, behind the Apps listener.
#[derive(Clone)]
pub struct ServerOrigins {
    catalog: HubCatalogHandle,
    hub: HubState,
    origins: Arc<AppOrigins>,
    auth: ProfileAuthState,
    issuer: HubInternalTokenIssuer,
    upstream: HubUpstreamHttpClientPool,
    /// The issuer's origin, where a person signs in.
    host: String,
}

impl ServerOrigins {
    pub fn new(
        catalog: HubCatalogHandle,
        hub: HubState,
        origins: AppOrigins,
        auth: ProfileAuthState,
        issuer: HubInternalTokenIssuer,
        upstream: HubUpstreamHttpClientPool,
    ) -> Self {
        Self {
            catalog,
            hub,
            host: origins.host().to_owned(),
            origins: Arc::new(origins),
            auth,
            issuer,
            upstream,
        }
    }

    /// `GET /app-origins/{slug}` on the issuer, where a person enters a server's app origin.
    pub fn entry_router(self) -> Router {
        Router::new()
            .route(
                &format!("{ENTRY_PATH}/{{slug}}"),
                axum::routing::get(
                    |State(origins): State<ServerOrigins>, uri: Uri, headers: HeaderMap| async move {
                        origins.enter(&uri, &headers).await
                    },
                ),
            )
            .with_state(self)
    }

    /// `GET /app-origins/{slug}?path=…&state=…` on the issuer: the person enters `slug`'s
    /// app origin. Signed in and allowed by the server's `app_origin` requirement, as a
    /// request on the origin is, they get a code for the browser holding `state`, redeemed
    /// at the app origin's [`SESSION_PATH`].
    async fn enter(&self, uri: &Uri, headers: &HeaderMap) -> Response {
        let not_here = || {
            refused(
                StatusCode::NOT_FOUND,
                "No app here",
                "This address names no app.",
            )
        };
        let origins = &self.origins;
        let Some(server) = uri
            .path()
            .strip_prefix(&format!("{ENTRY_PATH}/"))
            .and_then(|slug| ServerSlug::parse(slug).ok())
            .filter(|server| origins.app_server(&origins.app_origin(server)).is_some())
        else {
            return not_here();
        };
        let Some((return_to, state)) = uri.query().and_then(entry_query) else {
            return not_here();
        };
        let catalog = self.catalog.current();
        if catalog
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.computers.as_ref())
            .is_none()
        {
            return refused(
                StatusCode::NOT_FOUND,
                "No app here",
                "This installation signs no one in to apps.",
            );
        }
        let (config, session) =
            match crate::computers::app_session(&self.hub, &catalog, headers).await {
                Ok(Some(admitted)) => admitted,
                Ok(None) if bridge::document_navigation(headers) => {
                    let entry = entry_url("", &server, &return_to, &state);
                    return crate::computers::sign_in_for_entry(&entry)
                        .unwrap_or_else(|error| unavailable(&error));
                }
                Ok(None) => {
                    return private(
                        Problem::new(
                            StatusCode::UNAUTHORIZED,
                            "apps-sign-in-required",
                            "Sign-in required",
                        )
                        .into_response(),
                    );
                }
                Err(error) => return unavailable(&error),
            };
        if !declares_origin(&catalog, &server) {
            return not_here();
        }
        let request = Request::get("/")
            .body(Body::empty())
            .expect("a fixed request");
        let decided = self
            .as_person(
                &catalog,
                &config,
                &session,
                &server,
                request,
                Admission::Decide,
            )
            .await;
        if decided.status() != StatusCode::NO_CONTENT {
            return decided;
        }
        let code = match opaque() {
            Ok(code) => code,
            Err(error) => return unavailable(&error),
        };
        if let Err(error) = self
            .hub
            .hub_store()
            .insert_app_origin_code(
                &digest(&code),
                &state,
                &server,
                &session.computer().id,
                session.family(),
                &return_to,
                Utc::now() + TimeDelta::seconds(CODE_SECONDS),
            )
            .await
        {
            return unavailable(&error);
        }
        see_other(&format!(
            "{}{SESSION_PATH}?code={code}",
            origins.app_origin(&server)
        ))
    }

    /// Run `request` as the person of `session`: a bearer of the session's family passes
    /// the Hub's own profile authentication in process, as an MCP request's does, and the
    /// authenticated subject is admitted to `server`'s app origin, then, with
    /// [`Admission::Forward`], forwarded to the instance it selected. [`Admission::Decide`]
    /// answers 204 once admitted.
    async fn as_person(
        &self,
        catalog: &crate::HubCatalog,
        config: &crate::control_plane::Computers,
        session: &crate::computers::BrowserSession,
        server: &ServerSlug,
        request: Request,
        admission: Admission,
    ) -> Response {
        let token = match bridge::family_bearer(&self.hub, catalog, config, session, BEARER_SECONDS)
            .await
        {
            Ok(token) => token,
            Err(error) => return unavailable(&error),
        };
        let (mut parts, body) = request.into_parts();
        match HeaderValue::from_str(&format!("Bearer {}", token.access_token)) {
            Ok(bearer) => parts.headers.insert(header::AUTHORIZATION, bearer),
            Err(error) => return unavailable(&error),
        };
        let profile = config.profile.clone();
        let auth = self.auth.clone();
        let forward = Forward {
            origins: self.clone(),
            server: server.clone(),
            profile: profile.clone(),
            admission,
        };
        Router::new()
            .fallback(forwarded)
            .with_state(forward)
            .layer(axum::middleware::from_fn(move |request, next| {
                let auth = auth.clone();
                let profile = profile.clone();
                async move {
                    crate::http::authenticate_for_profile(&auth, &profile, request, next).await
                }
            }))
            .oneshot(Request::from_parts(parts, body))
            .await
            .unwrap_or_else(|never| match never {})
    }

    /// The server whose app origin a request names in `X-Forwarded-Host`: exactly one
    /// value, the `{slug}.{apps domain}` with the installation's port of a server whose
    /// registration declares an app origin.
    pub fn server(&self, headers: &HeaderMap) -> Option<ServerSlug> {
        let mut values = headers.get_all("x-forwarded-host").iter();
        let (Some(value), None) = (values.next(), values.next()) else {
            return None;
        };
        let host = value.to_str().ok()?.to_ascii_lowercase();
        let scheme = self.origins.sandbox().split_once("://")?.0;
        self.origins
            .app_server(&format!("{scheme}://{host}"))
            .filter(|server| declares_origin(&self.catalog.current(), server))
    }

    /// One request on `server`'s app origin.
    pub async fn serve(&self, server: ServerSlug, request: Request) -> Response {
        if request.uri().path() == SESSION_PATH {
            let (parts, _) = request.into_parts();
            return self
                .redeem(&server, &parts.method, &parts.uri, &parts.headers)
                .await;
        }
        let catalog = self.catalog.current();
        let Some(config) = catalog
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.computers.as_ref())
            .map(|computers| computers.config().clone())
        else {
            return refused(
                StatusCode::NOT_FOUND,
                "No app here",
                "This installation signs no one in to apps.",
            );
        };
        let session = match cookie(request.headers(), SESSION_COOKIE) {
            Some(value) => match self
                .hub
                .app_origin_browser_session(&catalog, &config, &digest(&value), &server)
                .await
            {
                Ok(session) => session,
                Err(error) => return unavailable(&error),
            },
            None => None,
        };
        let Some(session) = session else {
            return self.sign_in(&server, &request);
        };
        if request
            .headers()
            .get(header::UPGRADE)
            .is_some_and(|value| !value.is_empty())
        {
            return Problem::new(
                StatusCode::NOT_IMPLEMENTED,
                "app-origin-upgrade",
                "App origins forward no protocol upgrade",
            )
            .into_response();
        }
        self.as_person(
            &catalog,
            &config,
            &session,
            &server,
            request,
            Admission::Forward,
        )
        .await
    }

    /// No admitted session: a page load goes to the issuer to sign in, holding new state;
    /// anything else is refused.
    fn sign_in(&self, server: &ServerSlug, request: &Request) -> Response {
        if request.method() != Method::GET || !bridge::document_navigation(request.headers()) {
            return private(
                Problem::new(
                    StatusCode::UNAUTHORIZED,
                    "app-origin-sign-in-required",
                    "Sign-in required",
                )
                .detail("Open the app's page to sign in.")
                .into_response(),
            );
        }
        let return_to = request
            .uri()
            .path_and_query()
            .map(|path| path.as_str())
            .filter(|path| return_path(path))
            .unwrap_or("/");
        let state = match opaque() {
            Ok(state) => state,
            Err(error) => return unavailable(&error),
        };
        let mut response = see_other(&entry_url(&self.host, server, return_to, &digest(&state)));
        response.headers_mut().append(
            header::SET_COOKIE,
            set_cookie(STATE_COOKIE, &state, STATE_SECONDS),
        );
        response
    }

    /// `GET /_veoveo/session?code=…`: the browser that holds the code's state redeems it
    /// once for a session of the person on this app origin.
    async fn redeem(
        &self,
        server: &ServerSlug,
        method: &Method,
        uri: &Uri,
        headers: &HeaderMap,
    ) -> Response {
        let expired = || {
            refused(
                StatusCode::BAD_REQUEST,
                "Sign-in expired",
                "This sign-in did not complete in this browser in time. Open the app again.",
            )
        };
        if method != Method::GET {
            return expired();
        }
        let code = uri.query().and_then(|query| {
            let mut pairs = url::form_urlencoded::parse(query.as_bytes());
            match (pairs.next(), pairs.next()) {
                (Some((key, value)), None) if key == "code" && opaque_value(&value) => {
                    Some(value.into_owned())
                }
                _ => None,
            }
        });
        let (Some(code), Some(state)) = (code, cookie(headers, STATE_COOKIE)) else {
            return expired();
        };
        let store = self.hub.hub_store();
        let redeemed = match store
            .consume_app_origin_code(&digest(&code), &digest(&state), server)
            .await
        {
            Ok(Some(redeemed)) => redeemed,
            Ok(None) => return expired(),
            Err(error) => return unavailable(&error),
        };
        let family = match store.refresh_family(&redeemed.family).await {
            Ok(Some(family)) if family.revoked_at.is_none() && family.expires_at > Utc::now() => {
                family
            }
            Ok(_) => return expired(),
            Err(error) => return unavailable(&error),
        };
        let session = match opaque() {
            Ok(session) => session,
            Err(error) => return unavailable(&error),
        };
        if let Err(error) = store
            .insert_app_origin_session(&digest(&session), server, &redeemed.computer, &family.id)
            .await
        {
            return unavailable(&error);
        }
        let mut response = see_other(&redeemed.return_path);
        let headers = response.headers_mut();
        headers.append(
            header::SET_COOKIE,
            set_cookie(
                SESSION_COOKIE,
                &session,
                (family.expires_at - Utc::now()).num_seconds().max(0),
            ),
        );
        headers.append(header::SET_COOKIE, set_cookie(STATE_COOKIE, "", 0));
        response
    }
}

/// What an authenticated request on an app origin does once the person is admitted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Admission {
    /// Answer 204: the entry only decides.
    Decide,
    /// Forward the request to the instance the person selected.
    Forward,
}

#[derive(Clone)]
struct Forward {
    origins: ServerOrigins,
    server: ServerSlug,
    profile: veoveo_hub_sdk::HubProfileId,
    admission: Admission,
}

/// The authenticated request on the app origin: admitted to the person's execution, it
/// reaches the server's authenticated routes on the instance they selected, when the
/// server's `app_origin` requirement allows them.
async fn forwarded(State(forward): State<Forward>, request: Request) -> Response {
    let Forward {
        origins,
        server,
        profile,
        admission,
    } = forward;
    let Some(mut subject) = request.extensions().get::<AuthenticatedSubject>().cloned() else {
        return Problem::internal().into_response();
    };
    let snapshot = origins.catalog.snapshot();
    let admitted =
        match crate::execution::admit_execution(&origins.hub, snapshot.catalog(), &mut subject)
            .await
        {
            Ok(admitted) => admitted,
            Err(error) => return unavailable(&error),
        };
    let catalog = snapshot.catalog();
    let selected = snapshot.selected(&admitted.composition).current();
    let Some(registered) = selected.server(&server).cloned().filter(|registered| {
        registered
            .surfaces
            .as_ref()
            .is_some_and(|surfaces| surfaces.app_origin)
    }) else {
        return refused(
            StatusCode::NOT_FOUND,
            "No app here",
            "The instance you selected for this server serves no app origin.",
        );
    };
    let target = PolicyTarget::AppOrigin {
        server: server.clone(),
    };
    let managed = match origins
        .hub
        .oauth_action_admitted(catalog, &subject, HubAction::AppOrigin, &target)
        .await
    {
        Ok(managed) => managed,
        Err(error) => return unavailable(&error),
    };
    let decision = catalog.decide(crate::PolicyRequest {
        principal: &subject.principal,
        profile: &profile,
        action: HubAction::AppOrigin.into(),
        target: &target,
        trace_id: &trace_id(),
    });
    if !managed || decision.effect != PolicyEffect::Allow {
        tracing::info!(%server, reason = ?decision.reason, managed, "app origin request refused");
        return refused(
            StatusCode::FORBIDDEN,
            "Not allowed",
            &format!("You may not open {server}'s app."),
        );
    }
    if admission == Admission::Decide {
        return StatusCode::NO_CONTENT.into_response();
    }
    let permissions = crate::policy::granted_permissions(
        &catalog.control_plane().authorization,
        &registered.authorization,
        &subject.principal,
    );
    let bearer = match crate::mcp::forwarded_bearer(
        &origins.issuer,
        profile,
        &registered,
        &subject,
        permissions,
        i64::from(BEARER_SECONDS),
    ) {
        Ok(bearer) => bearer,
        Err(error) => return unavailable(&error),
    };
    // The catalog's pool carries the native transport to factory sandboxes, as MCP's does.
    let pool = catalog.upstream_http().unwrap_or(&origins.upstream);
    let client = match pool.http_client(
        registered.deployment,
        &registered.endpoint,
        registered.branch.is_some(),
    ) {
        Ok(client) => client,
        Err(error) => return unavailable(&error),
    };
    let (parts, body) = request.into_parts();
    let mut url = registered.endpoint.url().clone();
    url.set_path(parts.uri.path());
    url.set_query(parts.uri.query());
    let mut headers = parts.headers;
    super::super::surfaces::strip_hop(&mut headers);
    strip_hub_cookies(&mut headers);
    let host = origins.origins.app_origin(&server);
    let host = host
        .split_once("://")
        .map_or(host.as_str(), |(_, host)| host);
    let (Ok(host), Ok(bearer)) = (
        HeaderValue::from_str(host),
        HeaderValue::from_str(&format!("Bearer {bearer}")),
    ) else {
        return Problem::internal().into_response();
    };
    headers.insert(header::HOST, host);
    headers.insert(header::AUTHORIZATION, bearer);
    let answer = match client
        .request(parts.method, url)
        .headers(headers)
        .body(reqwest::Body::wrap_stream(body.into_data_stream()))
        .send()
        .await
    {
        Ok(answer) => answer,
        Err(error) => {
            tracing::warn!(%server, error = %error, "an app origin's server did not answer");
            return refused(
                StatusCode::BAD_GATEWAY,
                "App unavailable",
                "The app's server did not answer.",
            );
        }
    };
    let mut response = Response::builder().status(answer.status());
    if let Some(headers) = response.headers_mut() {
        *headers = answer.headers().clone();
        super::super::surfaces::strip_hop(headers);
        strip_hub_set_cookies(headers);
    }
    response
        .body(Body::from_stream(answer.bytes_stream()))
        .unwrap_or_else(|_| Problem::internal().into_response())
}

/// Whether a cookie name is one of the Hub's on an app origin.
fn hub_cookie(name: &str) -> bool {
    name == SESSION_COOKIE || name == STATE_COOKIE
}

/// Remove the Hub's cookies from a request, keeping the application's own.
fn strip_hub_cookies(headers: &mut HeaderMap) {
    let kept: Vec<String> = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .map(str::trim)
        .filter(|item| {
            !item.is_empty() && !hub_cookie(item.split_once('=').map_or(*item, |(name, _)| name))
        })
        .map(str::to_owned)
        .collect();
    headers.remove(header::COOKIE);
    if !kept.is_empty()
        && let Ok(value) = HeaderValue::from_str(&kept.join("; "))
    {
        headers.insert(header::COOKIE, value);
    }
}

/// Remove an application's attempts to set the Hub's cookies, keeping its own.
fn strip_hub_set_cookies(headers: &mut HeaderMap) {
    let kept: Vec<HeaderValue> = headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter(|value| {
            value.to_str().ok().is_some_and(|value| {
                let name = value.split_once('=').map_or(value, |(name, _)| name).trim();
                !hub_cookie(name)
            })
        })
        .cloned()
        .collect();
    headers.remove(header::SET_COOKIE);
    for value in kept {
        headers.append(header::SET_COOKIE, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_return_path_stays_on_the_app_origin() {
        for path in ["/", "/workflows?id=1", "/a/b#c"] {
            assert!(return_path(path), "{path}");
        }
        for path in ["", "workflows", "//evil.example/", "/\\evil", "/a\r\nb"] {
            assert!(!return_path(path), "{path}");
        }
    }

    #[test]
    fn an_entry_respells_canonically_and_refuses_anything_else() {
        let state = digest("state");
        let entry = entry_url("", &ServerSlug::parse("n8n").unwrap(), "/home?x=1", &state);
        assert_eq!(entry_from_path(&entry).as_deref(), Some(entry.as_str()));
        assert!(entry.starts_with("/app-origins/n8n?path=%2Fhome%3Fx%3D1&state=sha256%3A"));
        for path in [
            "/app-origins/n8n?path=%2F".to_owned(),
            format!("/app-origins/n8n?path=%2F%2Fevil&state={}", state.as_str()),
            format!(
                "/app-origins/n8n?path=%2F&state={}&state={}",
                state.as_str(),
                state.as_str()
            ),
            format!("/app-origins/n8n?path=%2F&state={}&other=1", state.as_str()),
            format!("/app-origins/N8N!?path=%2F&state={}", state.as_str()),
            "/apps/time/now".to_owned(),
        ] {
            assert_eq!(entry_from_path(&path), None, "{path}");
        }
    }

    #[test]
    fn the_hubs_cookies_never_cross_to_or_from_the_application() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("n8n-auth=a; __Host-veoveo-app=b; theme=dark"),
        );
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("__Host-veoveo-app-state=c"),
        );
        strip_hub_cookies(&mut headers);
        assert_eq!(headers[header::COOKIE], "n8n-auth=a; theme=dark");
        let mut headers = HeaderMap::new();
        headers.append(
            header::SET_COOKIE,
            HeaderValue::from_static("n8n-auth=x; Path=/"),
        );
        headers.append(
            header::SET_COOKIE,
            HeaderValue::from_static("__Host-veoveo-app=y; Path=/"),
        );
        strip_hub_set_cookies(&mut headers);
        let set: Vec<_> = headers.get_all(header::SET_COOKIE).iter().collect();
        assert_eq!(set, ["n8n-auth=x; Path=/"]);
    }
}
