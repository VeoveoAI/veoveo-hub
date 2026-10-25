//! Raw HTTP/upgrade transport over native authenticated ForwardTcp. No decoding,
//! credential broker, arbitrary native destination, or positive auth cache.
use std::{
    pin::Pin,
    task::{Context, Poll},
};

use anyhow::{Context as _, Result, ensure};
use axum::{
    body::{Body, Bytes},
    extract::Request,
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Redirect, Response},
};
use http_body::{Body as HttpBody, Frame, SizeHint};
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use tokio_util::sync::CancellationToken;
use veoveo_types::Sha256Digest;

use super::{
    ADMISSION_READ_TIMEOUT, BrowserSession, ComputerId, ComputerRuntime, STREAM_ADMISSION_INTERVAL,
    gateway::ForwardError,
    oauth::{SESSION_COOKIE, cookie, denied, digest, private, unavailable},
    provisioning::{Provisioner, phase},
};
use openshell_sdk::raw::proto as p;

/// A page load of a Computer whose sandbox is not ready to serve. Entry creates or
/// awaits it on the start page.
#[derive(Debug, thiserror::Error)]
#[error("the Computer's sandbox is not ready; entry starts it")]
pub(super) struct Reenter;

/// A page load of a Computer whose last replacement failed or whose sandbox the Hub can
/// no longer serve safely. The status page offers Restart.
#[derive(Debug, thiserror::Error)]
#[error("the Computer needs its owner: {0}")]
pub(super) struct Attention(String);

/// The owner's browser admission ended while a request was being forwarded.
#[derive(Debug, thiserror::Error)]
#[error("Computer admission ended {0}")]
struct AdmissionEnded(&'static str);

/// How the Hub logs a Computer request it answers unavailable itself. Its answer is
/// marked expected, so the HTTP trace layer does not log it again as a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unforwarded {
    /// Planned: the owner's admission ended, or the Hub is replacing, starting or
    /// stopping the sandbox or has collected it. INFO.
    Planned(&'static str),
    /// An operator should notice: forwarding capacity is exhausted, the Computer needs
    /// its owner, or its ready sandbox does not answer. WARN.
    Notable(&'static str),
    /// A failure of the Hub: it cannot read the Computer's state, or the gateway
    /// refuses its authentication or configuration. ERROR.
    Failed(&'static str),
}

/// What the Hub's record and the gateway say about a Computer whose request failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Serving {
    /// A replacement, collection or first start runs now.
    Replacing,
    /// No sandbox, or one that is starting, stopping, stopped or being deleted.
    NotRunning,
    /// The last replacement failed, the sandbox is in error, or the Hub can no longer
    /// serve it safely.
    NeedsOwner,
    /// The sandbox is ready.
    Ready,
}

fn serving(observed: &super::lifecycle::Observed) -> Serving {
    if observed.operation().is_some() {
        return Serving::Replacing;
    }
    if observed.blocked.is_some() || observed.computer.failure.is_some() {
        return Serving::NeedsOwner;
    }
    match observed.phase() {
        Some(p::SandboxPhase::Ready) => Serving::Ready,
        None
        | Some(
            p::SandboxPhase::Provisioning
            | p::SandboxPhase::Stopping
            | p::SandboxPhase::Stopped
            | p::SandboxPhase::Completed
            | p::SandboxPhase::Deleting,
        ) => Serving::NotRunning,
        Some(_) => Serving::NeedsOwner,
    }
}

/// The cause `error` itself names for keeping a request from its Computer, if any.
fn named_cause(error: &anyhow::Error) -> Option<Unforwarded> {
    if error.downcast_ref::<AdmissionEnded>().is_some() {
        return Some(Unforwarded::Planned("the owner's admission ended"));
    }
    match error.downcast_ref::<ForwardError>()? {
        ForwardError::Capacity => Some(Unforwarded::Notable(
            "Computer forwarding capacity is exhausted",
        )),
        ForwardError::Authentication | ForwardError::Configuration => Some(Unforwarded::Failed(
            "the gateway refused the Hub's forwarding",
        )),
        ForwardError::Refused(_) | ForwardError::Transport | ForwardError::Deadline => None,
    }
}

/// The cause of an unforwarded request from what the Hub observed of its Computer after
/// the failure; `None` when the Hub could not observe it.
fn observed_cause(serving: Option<Serving>) -> Unforwarded {
    match serving {
        Some(Serving::Replacing) => Unforwarded::Planned("the Hub is replacing its sandbox"),
        Some(Serving::NotRunning) => Unforwarded::Planned("its sandbox is not running"),
        Some(Serving::NeedsOwner) => Unforwarded::Notable("the Computer needs its owner"),
        Some(Serving::Ready) => Unforwarded::Notable("its ready sandbox did not answer"),
        None => Unforwarded::Failed("the Hub could not read the Computer's state"),
    }
}

impl Unforwarded {
    fn log(self, computer: &ComputerId, error: &anyhow::Error) {
        match self {
            Self::Planned(reason) => tracing::info!(
                %computer, reason, error = format!("{error:#}"),
                "Computer request answered unavailable"
            ),
            Self::Notable(reason) => tracing::warn!(
                %computer, reason, error = format!("{error:#}"),
                "Computer request answered unavailable"
            ),
            Self::Failed(reason) => tracing::error!(
                %computer, reason, error = format!("{error:#}"),
                "Computer request answered unavailable"
            ),
        }
    }
}

type BodyError = Box<dyn std::error::Error + Send + Sync>;

pub(super) fn same_origin(runtime: &ComputerRuntime, headers: &HeaderMap) -> bool {
    origin_matches(headers, &runtime.public.origin().ascii_serialization())
}
fn origin_matches(headers: &HeaderMap, origin: &str) -> bool {
    let mut values = headers.get_all(header::ORIGIN).iter();
    matches!((values.next(), values.next()), (Some(value), None) if value.as_bytes() == origin.as_bytes())
}
fn websocket(request: &Request) -> bool {
    request.method() == Method::GET
        && request
            .headers()
            .get(header::UPGRADE)
            .is_some_and(|v| v.as_bytes().eq_ignore_ascii_case(b"websocket"))
}
/// A browser's top-level page load: a GET the browser marks as a navigation, of a
/// document when it names the destination.
fn document_navigation(request: &Request) -> bool {
    request.method() == Method::GET
        && !websocket(request)
        && request
            .headers()
            .get("sec-fetch-mode")
            .is_some_and(|value| value == "navigate")
        && request
            .headers()
            .get("sec-fetch-dest")
            .is_none_or(|value| value == "document")
}
/// Entry resolves the current Computer, starting it, or starts ordinary sign-in. Never
/// carry a stale mount, session path or caller-supplied return URL forward.
fn reenter() -> Response {
    private(Redirect::to(super::pages::ENTRY).into_response())
}
fn authorization_required(request: &Request) -> Response {
    if document_navigation(request) {
        reenter()
    } else {
        denied()
    }
}
/// A page load of a Computer that is not ready goes back to entry, which starts it on
/// the start page, and one that needs its owner to the status page; anything else, and
/// every other request, answers unavailable. The caller logs why the request was not
/// forwarded, so the unavailable answer is marked expected.
fn unavailable_for(navigation: bool, error: &anyhow::Error) -> Response {
    if navigation && redirected(error) {
        if error.downcast_ref::<Reenter>().is_some() {
            reenter()
        } else {
            private(Redirect::to(super::pages::STATUS).into_response())
        }
    } else {
        crate::http::expected(unavailable())
    }
}
/// Whether a page load that failed with `error` goes back to entry or to the status page.
fn redirected(error: &anyhow::Error) -> bool {
    error.downcast_ref::<Reenter>().is_some() || error.downcast_ref::<Attention>().is_some()
}

/// A page load serves only a ready sandbox the Hub can serve safely, awaiting one that
/// OpenShell is still starting. One that is missing, stopped or being replaced goes
/// back to entry.
async fn ready_for_navigation(
    runtime: &ComputerRuntime,
    provisioner: &Provisioner,
    session: &BrowserSession,
) -> Result<()> {
    let observed = runtime.observe(session.computer()).await?;
    if observed.operation().is_some() {
        return Err(Reenter.into());
    }
    if let Some(blocked) = observed.blocked {
        return Err(Attention(blocked).into());
    }
    match observed.sandbox.as_ref().map(phase) {
        Some(p::SandboxPhase::Ready) => Ok(()),
        None
        | Some(p::SandboxPhase::Stopped | p::SandboxPhase::Completed | p::SandboxPhase::Deleting) =>
        {
            if let Some(failure) = observed.computer.failure {
                return Err(Attention(failure.cause).into());
            }
            Err(Reenter.into())
        }
        Some(_) => provisioner
            .wait_phase(&session.computer().native_name(), p::SandboxPhase::Ready)
            .await
            .map_err(|_| Reenter.into()),
    }
}
/// Connection-nominated fields are hop-by-hop too. Keep content encoding/length:
/// hyper forwards the original encoded entity bytes and owns fresh framing.
fn strip_hop(headers: &mut HeaderMap) {
    let nominated = headers
        .get_all(header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|value| HeaderName::from_bytes(value.trim().as_bytes()).ok())
        .collect::<Vec<_>>();
    for name in nominated {
        headers.remove(name);
    }
    for name in [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ] {
        headers.remove(name);
    }
}
fn request_headers(headers: &mut HeaderMap, host: &HeaderValue, upgrade: bool) {
    strip_hop(headers);
    for name in [
        "authorization",
        "cookie",
        "origin",
        "forwarded",
        "x-forwarded-for",
        "x-forwarded-host",
        "x-forwarded-proto",
        "x-real-ip",
    ] {
        headers.remove(name);
    }
    headers.insert(header::HOST, host.clone());
    if upgrade {
        headers.insert(header::CONNECTION, HeaderValue::from_static("upgrade"));
        headers.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
    }
}
fn response_headers(headers: &mut HeaderMap, upgrade: bool) {
    strip_hop(headers);
    headers.remove(header::SET_COOKIE);
    let cors = headers
        .keys()
        .filter(|name| name.as_str().starts_with("access-control-"))
        .cloned()
        .collect::<Vec<_>>();
    for name in cors {
        headers.remove(name);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    if upgrade {
        headers.insert(header::CONNECTION, HeaderValue::from_static("upgrade"));
        headers.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
    }
}
fn route(uri: &Uri) -> Option<(ComputerId, String)> {
    // Inspect the raw path, not an extractor's decoded alias. The native app
    // receives its own original path/query after the exact owner prefix.
    let raw = uri.path().strip_prefix("/computer/c/")?;
    let (id, tail) = raw.split_once('/').unwrap_or((raw, ""));
    if id.contains('%') {
        return None;
    }
    let id = ComputerId::parse(id).ok()?;
    let path = format!(
        "/{tail}{}",
        uri.query().map(|q| format!("?{q}")).unwrap_or_default()
    );
    Some((id, path))
}
struct ConnectionGuard {
    cancel: CancellationToken,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.cancel.cancel();
        for task in &self.tasks {
            task.abort();
        }
    }
}
struct GuardedBody {
    body: Incoming,
    guard: ConnectionGuard,
}
impl HttpBody for GuardedBody {
    type Data = Bytes;
    type Error = BodyError;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        if self.guard.cancel.is_cancelled() {
            return Poll::Ready(Some(Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Computer admission ended",
            )
            .into())));
        }
        Pin::new(&mut self.body)
            .poll_frame(context)
            .map(|frame| frame.map(|result| result.map_err(|error| Box::new(error) as BodyError)))
    }
    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }
    fn size_hint(&self) -> SizeHint {
        self.body.size_hint()
    }
}

pub(super) async fn forward(runtime: ComputerRuntime, mut request: Request) -> Response {
    let Some((id, path)) = route(request.uri()) else {
        return private(crate::problem::Problem::not_found().into_response());
    };
    let upgrade = websocket(&request);
    let navigation = document_navigation(&request);
    if (upgrade
        || !matches!(
            *request.method(),
            Method::GET | Method::HEAD | Method::OPTIONS
        ))
        && !same_origin(&runtime, request.headers())
    {
        return private(
            crate::problem::Problem::new(
                StatusCode::FORBIDDEN,
                "computer-origin-refused",
                "Computer origin refused",
            )
            .into_response(),
        );
    }
    let Ok(Some(value)) = cookie(request.headers(), SESSION_COOKIE) else {
        return authorization_required(&request);
    };
    let cookie_digest = digest(&value);
    let session = match runtime
        .hub
        .computer_browser_session(
            &runtime.catalog.current(),
            &runtime.config,
            &cookie_digest,
            Some(&id),
        )
        .await
    {
        Ok(Some(session)) => session,
        Ok(None) => return authorization_required(&request),
        Err(_) => return unavailable(),
    };
    // The owner's browser uses the Computer: it is not idle.
    runtime.browser.record(&id, chrono::Utc::now());
    // Canonical mount slash matters to native browser URL resolution.
    if request.uri().path() == session.computer().base_path().trim_end_matches('/') {
        let mut response = StatusCode::PERMANENT_REDIRECT.into_response();
        let location = format!(
            "{}{}",
            session.computer().base_path(),
            request
                .uri()
                .query()
                .map(|q| format!("?{q}"))
                .unwrap_or_default()
        );
        let Ok(location) = HeaderValue::from_str(&location) else {
            return unavailable();
        };
        response.headers_mut().insert(header::LOCATION, location);
        return private(response);
    }
    match stream(
        &runtime,
        &session,
        cookie_digest,
        &mut request,
        &path,
        upgrade,
        navigation,
    )
    .await
    {
        Ok(response) => response,
        Err(error) if navigation && redirected(&error) => {
            tracing::debug!(computer = %id, error = %error, "Computer page load redirected");
            unavailable_for(navigation, &error)
        }
        Err(error) => {
            let cause = match named_cause(&error) {
                Some(cause) => cause,
                None => observed_cause(
                    runtime
                        .observe(session.computer())
                        .await
                        .ok()
                        .as_ref()
                        .map(serving),
                ),
            };
            cause.log(&id, &error);
            unavailable_for(navigation, &error)
        }
    }
}
async fn stream(
    runtime: &ComputerRuntime,
    session: &BrowserSession,
    cookie: Sha256Digest,
    request: &mut Request,
    path: &str,
    upgrade: bool,
    navigation: bool,
) -> Result<Response> {
    let provisioner = runtime
        .installation
        .for_computer(session.computer())
        .context("Computer recorded configuration unavailable")?;
    if navigation {
        ready_for_navigation(runtime, &provisioner, session).await?;
    }
    // Fresh admission after a potentially slow native lifecycle operation.
    ensure!(
        runtime
            .hub
            .computer_browser_session(
                &runtime.catalog.current(),
                &runtime.config,
                &cookie,
                Some(&session.computer().id)
            )
            .await?
            .is_some(),
        AdmissionEnded("before forwarding")
    );
    let (socket, forward) = if navigation {
        provisioner
            .gateway
            .stream_when_ready(
                session.computer().native_name(),
                provisioner.config.port,
                super::provisioning::READY_TIMEOUT,
            )
            .await?
    } else {
        provisioner
            .gateway
            .stream(session.computer().native_name(), provisioner.config.port)
            .await?
    };
    // A cold listener wait can outlast revocation/expiry. Re-admit before any
    // HTTP bytes leave the connected native transport, and close on refusal.
    match runtime
        .hub
        .computer_browser_session(
            &runtime.catalog.current(),
            &runtime.config,
            &cookie,
            Some(&session.computer().id),
        )
        .await
    {
        Ok(Some(current)) if current.family() == session.family() => {}
        _ => {
            drop(socket);
            forward.abort();
            return Err(AdmissionEnded("while connecting").into());
        }
    }
    let (mut sender, connection) =
        hyper::client::conn::http1::handshake(TokioIo::new(socket)).await?;
    let cancel = CancellationToken::new();
    let connection_cancel = cancel.clone();
    let connection = tokio::spawn(async move {
        tokio::select! { _ = connection.with_upgrades() => {}, _ = connection_cancel.cancelled() => {} }
    });
    let mut guard = ConnectionGuard {
        cancel: cancel.clone(),
        tasks: vec![forward, connection],
    };
    let forward_abort = guard.tasks[0].abort_handle();
    let connection_abort = guard.tasks[1].abort_handle();
    let monitor_runtime = runtime.clone();
    let monitor_session = session.clone();
    let monitor_cancel = cancel.clone();
    guard.tasks.push(tokio::spawn(async move {
        let expiry = (monitor_session.expires_at() - chrono::Utc::now()).to_std().unwrap_or_default();
        let deadline = tokio::time::sleep(expiry);
        tokio::pin!(deadline);
        let reads = async {
            loop {
                tokio::time::sleep(STREAM_ADMISSION_INTERVAL).await;
                let result = tokio::time::timeout(ADMISSION_READ_TIMEOUT, monitor_runtime.hub.computer_browser_session(&monitor_runtime.catalog.current(), &monitor_runtime.config, &cookie, Some(&monitor_session.computer().id))).await;
                if !matches!(result, Ok(Ok(Some(ref current))) if current.family() == monitor_session.family()) { break; }
            }
        };
        tokio::select! { _ = reads => {}, _ = &mut deadline => {}, _ = monitor_cancel.cancelled() => {} }
        monitor_cancel.cancel();
        forward_abort.abort();
        connection_abort.abort();
    }));
    let incoming_upgrade = upgrade.then(|| hyper::upgrade::on(&mut *request));
    let authority = runtime.public[url::Position::BeforeHost..url::Position::AfterPort].to_string();
    request_headers(
        request.headers_mut(),
        &HeaderValue::from_str(&authority)?,
        upgrade,
    );
    *request.uri_mut() = path.parse()?;
    let outgoing = std::mem::replace(request, Request::new(Body::empty()));
    let mut response =
        tokio::time::timeout(super::gateway::RPC_TIMEOUT, sender.send_request(outgoing)).await??;
    if upgrade {
        ensure!(
            response.status() == StatusCode::SWITCHING_PROTOCOLS,
            "native Computer upgrade refused"
        );
        let backend_upgrade = hyper::upgrade::on(&mut response);
        let (mut parts, _) = response.into_parts();
        response_headers(&mut parts.headers, true);
        let incoming_upgrade = incoming_upgrade.expect("upgrade selected");
        tokio::spawn(async move {
            let _guard = guard;
            let transfer = async {
                let (caller, backend) = tokio::try_join!(incoming_upgrade, backend_upgrade)?;
                tokio::io::copy_bidirectional(
                    &mut TokioIo::new(caller),
                    &mut TokioIo::new(backend),
                )
                .await?;
                Ok::<_, anyhow::Error>(())
            };
            tokio::select! { _ = transfer => {}, _ = cancel.cancelled() => {} }
        });
        Ok(Response::from_parts(parts, Body::empty()))
    } else {
        ensure!(
            response.status() != StatusCode::SWITCHING_PROTOCOLS,
            "unsolicited Computer upgrade"
        );
        let (mut parts, body) = response.into_parts();
        response_headers(&mut parts.headers, false);
        Ok(Response::from_parts(
            parts,
            Body::new(GuardedBody { body, guard }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authorization_refusal_recovers_documents_without_replaying_the_stale_url() {
        let request = Request::builder()
            .uri(format!(
                "/computer/c/{}/server/old-origin/session/old-session?return=https://other.example/",
                ComputerId::new()
            ))
            .header("sec-fetch-mode", "navigate")
            .header("sec-fetch-dest", "document")
            .body(Body::empty())
            .unwrap();
        let response = authorization_required(&request);
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()[header::LOCATION], "/computer");
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
    }

    #[test]
    fn authorization_refusal_keeps_fetch_api_and_websocket_responses_unauthorized() {
        for (method, mode, destination, upgrade) in [
            (Method::GET, None, None, false),
            (Method::GET, Some("cors"), Some("empty"), false),
            (Method::GET, Some("same-origin"), Some("script"), false),
            (Method::HEAD, Some("navigate"), Some("document"), false),
            (Method::POST, Some("navigate"), Some("document"), false),
            (Method::GET, Some("navigate"), Some("document"), true),
        ] {
            let mut request = Request::builder()
                .method(method)
                .uri(format!("/computer/c/{}/api/session", ComputerId::new()))
                .header(header::ACCEPT, "text/html");
            if let Some(mode) = mode {
                request = request.header("sec-fetch-mode", mode);
            }
            if let Some(destination) = destination {
                request = request.header("sec-fetch-dest", destination);
            }
            if upgrade {
                request = request.header(header::UPGRADE, "websocket");
            }
            let response = authorization_required(&request.body(Body::empty()).unwrap());
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert!(!response.headers().contains_key(header::LOCATION));
            assert_eq!(
                response.headers()[header::CACHE_CONTROL],
                "private, no-store"
            );
        }
    }

    #[test]
    fn only_a_page_load_goes_back_to_entry_or_status() {
        let missing = anyhow::Error::new(Reenter).context("enter the Computer");
        let response = unavailable_for(true, &missing);
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()[header::LOCATION], "/computer");
        let failed = anyhow::Error::new(Attention("synthetic cause".into()));
        let response = unavailable_for(true, &failed);
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()[header::LOCATION], "/computer/status");
        for (navigation, error) in [
            (false, anyhow::Error::new(Reenter)),
            (
                false,
                anyhow::Error::new(Attention("synthetic cause".into())),
            ),
            (true, anyhow::anyhow!("native Computer forwarding deadline")),
        ] {
            let response = unavailable_for(navigation, &error);
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert!(!response.headers().contains_key(header::LOCATION));
            // The proxy logs why; the trace layer does not log the answer as a failure.
            assert!(matches!(
                tower_http::classify::ClassifyResponse::classify_response(
                    crate::http::UnexpectedServerErrors,
                    &response
                ),
                tower_http::classify::ClassifiedResponse::Ready(Ok(()))
            ));
        }
    }

    #[test]
    fn a_planned_unavailability_is_never_an_error() {
        let named = |error: anyhow::Error| named_cause(&error);
        assert!(matches!(
            named(AdmissionEnded("while connecting").into()),
            Some(Unforwarded::Planned(_))
        ));
        assert!(matches!(
            named(ForwardError::Capacity.into()),
            Some(Unforwarded::Notable(_))
        ));
        for refused in [ForwardError::Authentication, ForwardError::Configuration] {
            assert!(matches!(
                named(refused.into()),
                Some(Unforwarded::Failed(_))
            ));
        }
        // A sandbox that cannot be reached, however the forwarding ended, is judged by
        // what the Hub observes of the Computer.
        for unreachable in [
            anyhow::Error::new(ForwardError::Refused(tonic::Code::Unavailable)),
            anyhow::Error::new(ForwardError::Transport),
            anyhow::Error::new(ForwardError::Deadline),
            anyhow::anyhow!("connection closed before message completed"),
        ] {
            assert_eq!(named(unreachable), None);
        }
        for serving in [Serving::Replacing, Serving::NotRunning] {
            assert!(matches!(
                observed_cause(Some(serving)),
                Unforwarded::Planned(_)
            ));
        }
        for serving in [Serving::NeedsOwner, Serving::Ready] {
            assert!(matches!(
                observed_cause(Some(serving)),
                Unforwarded::Notable(_)
            ));
        }
        assert!(matches!(observed_cause(None), Unforwarded::Failed(_)));
    }

    #[test]
    fn proxy_strips_credentials_nominated_hops_and_cors_without_decoding_entities() {
        let mut headers = HeaderMap::new();
        for (key, value) in [
            ("connection", "keep-alive, x-private-hop"),
            ("x-private-hop", "hidden"),
            ("authorization", "Bearer synthetic"),
            ("cookie", "synthetic=browser"),
            ("origin", "https://caller.example"),
            ("content-encoding", "br"),
            ("content-length", "128"),
            ("x-forwarded-host", "attacker.example"),
        ] {
            headers.insert(
                HeaderName::from_bytes(key.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        request_headers(
            &mut headers,
            &HeaderValue::from_static("public.example"),
            false,
        );
        for key in [
            "x-private-hop",
            "authorization",
            "cookie",
            "origin",
            "x-forwarded-host",
            "connection",
        ] {
            assert!(!headers.contains_key(key));
        }
        assert_eq!(headers[header::HOST], "public.example");
        assert_eq!(headers[header::CONTENT_ENCODING], "br");
        assert_eq!(headers[header::CONTENT_LENGTH], "128");
        headers.insert(
            header::SET_COOKIE,
            HeaderValue::from_static("backend=synthetic"),
        );
        headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
        response_headers(&mut headers, false);
        assert!(!headers.contains_key(header::SET_COOKIE));
        assert!(!headers.contains_key("access-control-allow-origin"));
        assert_eq!(headers[header::CACHE_CONTROL], "private, no-store");
        assert_eq!(headers[header::CONTENT_ENCODING], "br");
    }
    #[test]
    fn raw_owner_route_and_origin_refuse_aliases_and_ambiguity() {
        let id = ComputerId::new();
        let path = format!("/computer/c/{id}/api/session?cursor=opaque");
        let (got, forwarded) = route(&path.parse().unwrap()).unwrap();
        assert_eq!(got, id);
        assert_eq!(forwarded, "/api/session?cursor=opaque");
        assert!(route(&format!("/computer/c/%30{}/", id).parse().unwrap()).is_none());
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://public.example"),
        );
        assert!(origin_matches(&headers, "https://public.example"));
        assert!(!origin_matches(&headers, "https://public.example/"));
        headers.append(
            header::ORIGIN,
            HeaderValue::from_static("https://attacker.example"),
        );
        assert!(!origin_matches(&headers, "https://public.example"));
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use http_body_util::BodyExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Real HTTP parsing/framing over a duplex byte transport; the entity is
    /// synthetic bytes, not a claim about validity of its declared coding.
    #[tokio::test]
    async fn encoded_entity_bytes_and_content_length_survive_native_http_adapter() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let (caller, mut server) = tokio::io::duplex(4096);
            let fixture = tokio::spawn(async move {
                let mut header = Vec::new();
                while !header.ends_with(b"\r\n\r\n") {
                    ensure!(header.len() < 4096, "fixture request header bound");
                    header.push(server.read_u8().await?);
                }
                ensure!(header.starts_with(b"GET /api/example HTTP/1.1"), "fixture request path");
                server.write_all(b"HTTP/1.1 200 OK\r\nContent-Encoding: br\r\nContent-Length: 3\r\nConnection: close\r\n\r\n\x00\xff\x41").await?;
                Ok::<_, anyhow::Error>(())
            });
            let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(caller)).await.unwrap();
            let connection = tokio::spawn(async move { let _ = connection.await; });
            let response = sender.send_request(Request::builder().uri("/api/example").header(header::HOST, "public.example").body(Body::empty()).unwrap()).await.unwrap();
            let (mut parts, body) = response.into_parts();
            response_headers(&mut parts.headers, false);
            assert_eq!(parts.headers[header::CONTENT_ENCODING], "br");
            assert_eq!(parts.headers[header::CONTENT_LENGTH], "3");
            let guard = ConnectionGuard { cancel: CancellationToken::new(), tasks: vec![connection] };
            let body = Body::new(GuardedBody { body, guard }).collect().await.unwrap().to_bytes();
            assert_eq!(&body[..], b"\x00\xff\x41");
            fixture.await.unwrap().unwrap();
        }).await.expect("HTTP fixture deadline");
    }

    #[tokio::test]
    async fn cancelled_admission_ends_an_already_open_event_stream() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let (caller, mut server) = tokio::io::duplex(4096);
            let fixture = tokio::spawn(async move {
                let mut header = Vec::new();
                while !header.ends_with(b"\r\n\r\n") { header.push(server.read_u8().await?); }
                server.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\nb\r\ndata: one\n\n\r\n").await?;
                let mut closed = [0; 1];
                ensure!(server.read(&mut closed).await? == 0, "event stream transport not closed");
                Ok::<_, anyhow::Error>(())
            });
            let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(caller)).await.unwrap();
            let connection = tokio::spawn(async move { let _ = connection.await; });
            let response = sender.send_request(Request::builder().uri("/events").header(header::HOST, "public.example").body(Body::empty()).unwrap()).await.unwrap();
            let cancel = CancellationToken::new();
            let mut body = GuardedBody { body: response.into_body(), guard: ConnectionGuard { cancel: cancel.clone(), tasks: vec![connection] } };
            assert_eq!(body.frame().await.unwrap().unwrap().into_data().unwrap(), "data: one\n\n");
            cancel.cancel();
            assert!(body.frame().await.unwrap().is_err());
            drop(body);
            fixture.await.unwrap().unwrap();
        }).await.expect("event stream fixture deadline");
    }
}

#[cfg(test)]
mod forward_tests;
