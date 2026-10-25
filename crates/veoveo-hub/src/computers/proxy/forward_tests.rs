//! A Computer's page loads and requests through `forward`, against the fake OpenShell
//! gateway, which relays the sandbox's forwarding stream to a local HTTP server standing
//! in for the app, and a disposable database holding the owner's browser session. A
//! working Computer answers, and the Hub records the browser's request for collection. Once its sandbox is gone, a page load goes back to entry,
//! which creates it again at the current version on the start page, while the event
//! stream and a fetch keep answering unavailable. Every credential and answer is
//! synthetic.
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use openshell_sdk::raw::proto::{self as p, ProviderCredentialRefreshStrategy as Strategy};
use tower_http::classify::{ClassifiedResponse, ClassifyResponse};
use tracing_subscriber::{Layer, layer::SubscriberExt};

use super::forward;
use crate::computers::{
    api::status,
    oauth::{SESSION_COOKIE, digest, entry},
    test_fixture::{catalog, gateway, hub, installation, ready_computer, runtime},
};
use crate::http::UnexpectedServerErrors;

/// The level and reason of each "Computer request answered unavailable" line logged on
/// this thread while the capture's guard lives.
#[derive(Clone, Default)]
struct UnavailableLog(Arc<Mutex<Vec<(tracing::Level, String)>>>);

impl UnavailableLog {
    fn capture() -> (Self, tracing::subscriber::DefaultGuard) {
        let log = Self::default();
        let guard =
            tracing::subscriber::set_default(tracing_subscriber::registry().with(log.clone()));
        (log, guard)
    }

    fn take(&self) -> Vec<(tracing::Level, String)> {
        std::mem::take(&mut self.0.lock().unwrap())
    }
}

#[derive(Default)]
struct Fields {
    message: String,
    reason: String,
}

impl tracing::field::Visit for Fields {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "reason" {
            value.clone_into(&mut self.reason);
        }
    }
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        }
    }
}

impl<S: tracing::Subscriber> Layer<S> for UnavailableLog {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        if fields.message == "Computer request answered unavailable" {
            self.0
                .lock()
                .unwrap()
                .push((*event.metadata().level(), fields.reason));
        }
    }
}

/// The browser's session cookie, a synthetic random selector.
const COOKIE: &str = "5ad1c0de5ad1c0de5ad1c0de5ad1c0de5ad1c0de5ad1c0de5ad1c0de5ad1c0de";

/// A request a browser sends for `path` under the Computer: a page load, or an API
/// request with the destination a fetch or an event stream names.
fn request(
    id: &str,
    path: &str,
    mode: &str,
    destination: Option<&str>,
    accept: &str,
) -> Request<Body> {
    let mut request = Request::builder()
        .uri(format!("/computer/c/{id}{path}"))
        .header(header::COOKIE, format!("{SESSION_COOKIE}={COOKIE}"))
        .header(header::ACCEPT, accept)
        .header("sec-fetch-mode", mode);
    if let Some(destination) = destination {
        request = request.header("sec-fetch-dest", destination);
    }
    request.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn a_page_load_of_a_computer_without_its_sandbox_goes_back_to_entry() {
    tokio::time::timeout(Duration::from_secs(90), async {
        // The app inside the sandbox.
        let app = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = app.local_addr().unwrap();
        tokio::spawn(
            axum::serve(
                app,
                axum::Router::new().fallback(|| async { "synthetic app page" }),
            )
            .into_future(),
        );
        let (fake, provisioner) = gateway(Strategy::Oauth2ClientCredentials).await;
        fake.forward_to(address);
        let db = crate::test_store::TestDb::new().await;
        let hub = hub(&db);
        let catalog = catalog();
        // The owner's sign-in: custody, then the session bound to the browser. The
        // first entry creates the sandbox.
        let computer = ready_computer(&hub, &catalog, &provisioner, &digest(COOKIE)).await;
        provisioner
            .replace(&computer, uuid::Uuid::now_v7())
            .await
            .unwrap();
        let runtime = runtime(hub, catalog, installation(&provisioner));
        let id = computer.id.to_string();
        let page = || {
            request(
                &id,
                "/session/old",
                "navigate",
                Some("document"),
                "text/html",
            )
        };
        let fetch = || {
            request(
                &id,
                "/api/session",
                "cors",
                Some("empty"),
                "application/json",
            )
        };
        let events = || {
            request(
                &id,
                "/api/event",
                "cors",
                Some("empty"),
                "text/event-stream",
            )
        };

        // A working Computer answers its page loads and its requests, and each request
        // the Hub forwards keeps it from collection.
        assert!(runtime.browser.last(&computer.id).is_none());
        let before = chrono::Utc::now();
        let response = forward(runtime.clone(), page()).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            runtime
                .browser
                .last(&computer.id)
                .is_some_and(|at| at >= before)
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"synthetic app page");
        assert_eq!(
            forward(runtime.clone(), fetch()).await.status(),
            StatusCode::OK
        );

        // Its sandbox is deleted.
        let name = computer.native_name();
        let mut grpc = provisioner.gateway.client.raw_grpc_fresh().await.unwrap();
        grpc.delete_sandbox(p::DeleteSandboxRequest {
            workspace_scope: Some(p::workspace_selector(name.clone())),
            name: name.clone(),
            ..Default::default()
        })
        .await
        .unwrap();

        // A page load goes back to entry, also one whose browser names no destination.
        for page in [
            page(),
            request(&id, "/session/old", "navigate", None, "text/html"),
        ] {
            let response = forward(runtime.clone(), page).await;
            assert_eq!(response.status(), StatusCode::SEE_OTHER);
            assert_eq!(response.headers()[header::LOCATION], "/computer");
            assert_eq!(
                response.headers()[header::CACHE_CONTROL],
                "private, no-store"
            );
        }
        // The event stream, a fetch and a nested document keep answering unavailable,
        // which the Hub logs at INFO as planned: the sandbox is not running. The trace
        // layer does not log the answers as failures.
        let frame = request(&id, "/session/old", "navigate", Some("iframe"), "text/html");
        let (unavailable, logging) = UnavailableLog::capture();
        for request in [events(), fetch(), frame] {
            let response = forward(runtime.clone(), request).await;
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert!(!response.headers().contains_key(header::LOCATION));
            assert!(matches!(
                UnexpectedServerErrors.classify_response(&response),
                ClassifiedResponse::Ready(Ok(()))
            ));
        }
        drop(logging);
        assert_eq!(
            unavailable.take(),
            vec![
                (
                    tracing::Level::INFO,
                    "its sandbox is not running".to_owned()
                );
                3
            ]
        );

        // Entry, with the same session, creates the sandbox again at the current
        // version on the start page, which reloads itself, and then goes to the
        // Computer.
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("{SESSION_COOKIE}={COOKIE}").parse().unwrap(),
        );
        let response = entry(runtime.clone(), headers.clone()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let start = response.into_body().collect().await.unwrap().to_bytes();
        let start = std::str::from_utf8(&start).unwrap();
        assert!(start.contains("http-equiv=refresh"), "{start}");
        assert!(start.contains("Starting your Computer"), "{start}");
        let location = loop {
            let response = entry(runtime.clone(), headers.clone()).await;
            if response.status() == StatusCode::SEE_OTHER {
                break response.headers()[header::LOCATION]
                    .to_str()
                    .unwrap()
                    .to_owned();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert_eq!(location, computer.base_path());
        assert_eq!(
            forward(runtime.clone(), page()).await.status(),
            StatusCode::OK
        );
        assert!(fake.sandbox(&name, &name).is_some());

        // The status page names the version and offers Restart.
        let response = status(runtime.clone(), None, headers).await;
        assert_eq!(response.status(), StatusCode::OK);
        let status = response.into_body().collect().await.unwrap().to_bytes();
        let status = std::str::from_utf8(&status).unwrap();
        assert!(
            status.contains(computer.provisioning.version().unwrap().as_str()),
            "{status}"
        );
        assert!(status.contains("action=\"/computer/restart\""), "{status}");
        assert!(status.contains("Open your Computer"), "{status}");
    })
    .await
    .expect("the Computer forwarding test exceeded its bound");
}
