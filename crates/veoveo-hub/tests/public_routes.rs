//! Runs the Hub binary and a server that declares public routes, and checks over HTTP
//! that `/instances/{deployment}/{slug}/...` reaches that deployment within a declared
//! prefix only, without the issuer's cookies, under the public base's host, within the
//! prefix's body limit and rate.

#[path = "support/hub.rs"]
mod hub;

use std::{net::Ipv4Addr, time::Duration};

use axum::{
    Json, Router,
    body::Bytes,
    http::{HeaderMap, Method, Uri, header},
    response::IntoResponse,
    routing::any,
};
use hub::Hub;
use reqwest::StatusCode;
use secrecy::SecretString;
use veoveo_fixtures::store::TestDb;
use veoveo_hub_sdk::registration::{
    AuthorizationDeclaration, ClientMetadata, RegistrationDeclaration, ServerEndpoint,
    ServerVersion, client::RegistrationClient, statement::ServerKeyPair,
};
use veoveo_types::{ResourceScheme, ServerSlug};

const DEADLINE: Duration = Duration::from_secs(120);

#[tokio::test]
async fn a_public_route_reaches_its_deployment_within_its_prefix_body_limit_and_rate() {
    tokio::time::timeout(DEADLINE, qualify())
        .await
        .expect("the public route qualification exceeded its deadline");
}

/// What the server saw of a request, and a cookie it tries to set.
async fn echo(method: Method, uri: Uri, headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    (
        [(header::SET_COOKIE, "server=set; Path=/")],
        Json(serde_json::json!({
            "method": method.as_str(),
            "path": uri.path(),
            "query": uri.query(),
            "host": headers.get(header::HOST).and_then(|value| value.to_str().ok()),
            "cookie": headers.get(header::COOKIE).is_some(),
            "signature": headers.get("x-signature").and_then(|value| value.to_str().ok()),
            "body": String::from_utf8_lossy(&body),
        })),
    )
}

async fn qualify() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    let hub = Hub::start(directory.path(), &db, Duration::from_secs(60)).await;
    let http = reqwest::Client::new();

    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, Router::new().fallback(any(echo)))
            .await
            .unwrap();
    });

    let key = ServerKeyPair::generate().unwrap();
    let deployment = veoveo_hub_sdk::DeploymentId::new();
    let metadata = ClientMetadata::new(
        *key.public_key(),
        RegistrationDeclaration {
            deployment,
            slug: ServerSlug::parse("beacon").unwrap(),
            uri_scheme: ResourceScheme::parse("beacon").unwrap(),
            version: ServerVersion::new("0.1.0").unwrap(),
            endpoint: ServerEndpoint::new(
                url::Url::parse(&format!("http://{address}/beacon/mcp")).unwrap(),
            )
            .unwrap(),
            surfaces: Some(
                serde_json::from_value(serde_json::json!({"publicRoutes": [
                    {"prefix": "/beacon/hooks/", "maxBodyBytes": 16, "requestsPerMinute": 6000},
                    {"prefix": "/beacon/slow/", "maxBodyBytes": 16, "requestsPerMinute": 2}
                ]}))
                .unwrap(),
            ),
        },
        serde_json::from_value::<AuthorizationDeclaration>(
            serde_json::json!({ "permissions": [], "requirements": {} }),
        )
        .unwrap(),
    )
    .unwrap();
    let issuer = url::Url::parse(&hub.base).unwrap();
    let endpoint = RegistrationClient::discover(&http, &issuer).await.unwrap();
    let registered = RegistrationClient::new(
        http.clone(),
        endpoint,
        SecretString::from(hub.secret("VEOVEO_REGISTRATION_SECRET").to_owned()),
        key,
        metadata,
    )
    .unwrap()
    .register()
    .await
    .unwrap();
    let base = registered.public_base().unwrap().as_str().to_owned();
    assert_eq!(base, format!("{}/instances/{deployment}/beacon", hub.base));

    // The registration reaches the routing snapshot shortly after it is answered.
    let hook = format!("{base}/hooks/push?attempt=1");
    let answer = loop {
        let response = http
            .post(&hook)
            .header(header::COOKIE, "__Host-computer=session")
            .header("x-signature", "sha256=abc")
            .body("hello")
            .send()
            .await
            .unwrap();
        if response.status() != StatusCode::NOT_FOUND {
            break response;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(answer.status(), StatusCode::OK);
    assert!(answer.headers().get(header::SET_COOKIE).is_none());
    let seen: serde_json::Value = answer.json().await.unwrap();
    assert_eq!(
        seen,
        serde_json::json!({
            "method": "POST",
            "path": "/beacon/hooks/push",
            "query": "attempt=1",
            "host": hub.base.trim_start_matches("http://"),
            "cookie": false,
            "signature": "sha256=abc",
            "body": "hello",
        })
    );

    // A body over the prefix's limit, declared or streamed, is refused before the server.
    let response = http
        .post(&hook)
        .body("seventeen bytes!!")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    // Outside a declared prefix, with a dot segment or an encoded one, and for an unknown
    // deployment, nothing is routed.
    for path in [
        format!("{base}/mcp"),
        format!("{base}/hooks/../mcp"),
        format!("{base}/hooks/%2e%2e/mcp"),
        format!(
            "{}/instances/{}/beacon/hooks/push",
            hub.base,
            veoveo_hub_sdk::DeploymentId::new()
        ),
        format!("{}/instances/not-a-deployment/beacon/hooks/push", hub.base),
    ] {
        let response = http.get(&path).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }

    // Two requests a minute: the third waits.
    for _ in 0..2 {
        let response = http.get(format!("{base}/slow/a")).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    let response = http.get(format!("{base}/slow/a")).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()[header::RETRY_AFTER], "30");
}
