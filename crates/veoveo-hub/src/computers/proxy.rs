//! Raw HTTP/upgrade transport over native authenticated ForwardTcp. No decoding,
//! credential broker, arbitrary native destination, or positive auth cache.
use std::{
    pin::Pin,
    task::{Context, Poll},
};

use anyhow::{Result, ensure};
use axum::{
    body::{Body, Bytes},
    extract::Request,
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use http_body::{Body as HttpBody, Frame, SizeHint};
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use tokio_util::sync::CancellationToken;
use veoveo_types::Sha256Digest;

use super::{
    ADMISSION_READ_TIMEOUT, BrowserSession, ComputerId, ComputerRuntime, STREAM_ADMISSION_INTERVAL,
    oauth::{SESSION_COOKIE, cookie, denied, digest, private, unavailable},
};

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
        return denied();
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
        Ok(None) => return denied(),
        Err(_) => return unavailable(),
    };
    let mut runtime = runtime;
    runtime.provisioner = match runtime.provisioner.for_computer(session.computer()) {
        Ok(provisioner) => provisioner,
        Err(error) => {
            tracing::warn!(%error, "Computer recorded configuration unavailable");
            return unavailable();
        }
    };
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
    )
    .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::debug!(computer = %id, error = %error, "Computer transport unavailable");
            unavailable()
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
) -> Result<Response> {
    let navigation = request.method() == Method::GET
        && request
            .headers()
            .get("sec-fetch-mode")
            .is_some_and(|value| value == "navigate")
        && request
            .headers()
            .get("sec-fetch-dest")
            .is_some_and(|value| value == "document");
    if navigation {
        runtime.provisioner.resume(session.computer()).await?;
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
        "Computer admission ended before forwarding"
    );
    let (socket, forward) = if navigation {
        runtime
            .provisioner
            .gateway
            .stream_when_ready(
                session.computer().native_name(),
                runtime.provisioner.config.port,
                super::provisioning::READY_TIMEOUT,
            )
            .await?
    } else {
        runtime
            .provisioner
            .gateway
            .stream(
                session.computer().native_name(),
                runtime.provisioner.config.port,
            )
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
            anyhow::bail!("Computer admission ended while connecting");
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
