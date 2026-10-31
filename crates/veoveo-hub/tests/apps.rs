//! Runs the Hub binary with the Apps host configured and checks over HTTP that the Apps
//! listener serves the sandbox proxy and nothing else, whatever the request's `Host`,
//! under the App policy its query names; that the Hub's own listener never serves the
//! proxy page; and that without the installation's Computers, whose sign-in Apps use,
//! no App opens.

#[path = "support/hub.rs"]
mod hub;

use std::time::Duration;

use hub::Hub;
use reqwest::{StatusCode, header};
use veoveo_fixtures::store::TestDb;
use veoveo_hub_sdk::registration::{
    AuthorizationDeclaration, ClientMetadata, RegistrationDeclaration, ServerEndpoint,
    ServerVersion, client::RegistrationClient, statement::ServerKeyPair,
};
use veoveo_types::{ResourceScheme, ServerSlug};

const DEADLINE: Duration = Duration::from_secs(120);
const SANDBOX: &str = "https://sandbox.apps.veoveo.example";

#[tokio::test]
async fn the_apps_listener_serves_the_sandbox_proxy_and_nothing_else() {
    tokio::time::timeout(DEADLINE, qualify())
        .await
        .expect("the Apps listener qualification exceeded its deadline");
}

async fn qualify() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    let hub = Hub::start_with(
        directory.path(),
        &db,
        Duration::from_secs(60),
        &[
            ("VEOVEO_APPS_SANDBOX_ORIGIN", SANDBOX.to_owned()),
            ("VEOVEO_APPS_LISTEN_ADDRESS", "127.0.0.1:0".to_owned()),
            ("VEOVEO_APPS_LISTENS_PER_SESSION", "3".to_owned()),
        ],
        |_| {},
    )
    .await;
    let listening = hub.logged("apps listening");
    assert_eq!(listening.len(), 1, "one Apps listener");
    assert_eq!(listening[0]["sandbox"], SANDBOX);
    let apps = format!("http://{}", listening[0]["address"].as_str().unwrap());
    let http = reqwest::Client::new();

    // The proxy page, whatever the Host names, under the restrictive default policy and
    // framed only by the host page's origin.
    for host in [
        None,
        Some("sandbox.apps.veoveo.example"),
        Some(hub.base.trim_start_matches("http://")),
    ] {
        let mut request = http.get(format!("{apps}/"));
        if let Some(host) = host {
            request = request.header(header::HOST, host);
        }
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "Host {host:?}");
        assert_eq!(
            response.headers()[header::CONTENT_SECURITY_POLICY],
            format!(
                "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; \
                 img-src 'self' data:; font-src 'self'; media-src 'self' data:; connect-src 'none'; \
                 frame-src 'none'; base-uri 'self'; object-src 'none'; frame-ancestors {}",
                hub.base
            )
            .as_str()
        );
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
        let page = response.text().await.unwrap();
        assert!(
            page.contains(&format!("const host = \"{}\";", hub.base)),
            "{page}"
        );
        assert!(page.contains("app.setAttribute(\"sandbox\", \"allow-scripts\");"));
        assert!(
            !page.contains("allow-same-origin\")"),
            "the App's frame never gets allow-same-origin"
        );
        assert!(page.contains("ui/notifications/sandbox-"));
    }

    // The policy the query names, validated again.
    let response = http
        .get(format!(
            "{apps}/?connect=https%3A%2F%2Fapi.example.net&resource=https%3A%2F%2Fcdn.example.net&frame=https%3A%2F%2Ffixture.apps.veoveo.example"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let policy = response.headers()[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap()
        .to_owned();
    for directive in [
        "script-src 'self' 'unsafe-inline' https://cdn.example.net",
        "connect-src https://api.example.net",
        "frame-src https://fixture.apps.veoveo.example",
    ] {
        assert!(policy.contains(directive), "{policy}");
    }
    for query in [
        "frame=https%3A%2F%2Fwww.example.net",
        "frame=https%3A%2F%2Fsandbox.apps.veoveo.example",
        "connect=https%3A%2F%2F%2A.example.net",
        "connect=https%3A%2F%2Fsandbox.apps.veoveo.example",
        "script=https%3A%2F%2Fcdn.example.net",
    ] {
        let response = http.get(format!("{apps}/?{query}")).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
    }

    // The Apps listener reaches no other route; the proxy takes only GET.
    for path in [
        "/mcp/local",
        "/apps/fixture/board.html",
        "/apps/host.js",
        "/livez",
        "/.well-known/oauth-authorization-server",
        "/proxy",
    ] {
        let response = http.get(format!("{apps}{path}")).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
    let response = http.post(format!("{apps}/")).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    // The Hub's own listener never serves the proxy page, and refuses the sandbox's name.
    let response = http.get(format!("{}/", hub.base)).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = http
        .get(format!(
            "{}{}",
            hub.base,
            veoveo_hub::apps::host_script_path()
        ))
        .header(header::HOST, "sandbox.apps.veoveo.example")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::MISDIRECTED_REQUEST);

    // The host script is public at its digest's path; without Computers, whose sign-in
    // Apps use, no App opens.
    let response = http
        .get(format!(
            "{}{}",
            hub.base,
            veoveo_hub::apps::host_script_path()
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/javascript; charset=utf-8"
    );
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
    let response = http
        .get(format!("{}/apps/fixture/board.html", hub.base))
        .header("sec-fetch-mode", "navigate")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(
        response
            .text()
            .await
            .unwrap()
            .contains("signs no one in to Apps")
    );

    // A server that declares an app origin is told its origin under the apps domain.
    let key = ServerKeyPair::generate().unwrap();
    let authorization: AuthorizationDeclaration = serde_json::from_value(serde_json::json!({
        "permissions": [], "requirements": {"appOrigin": {"kind": "authenticated"}}
    }))
    .unwrap();
    let metadata = ClientMetadata::new(
        *key.public_key(),
        RegistrationDeclaration {
            deployment: veoveo_hub_sdk::DeploymentId::new(),
            slug: ServerSlug::parse("beacon").unwrap(),
            uri_scheme: ResourceScheme::parse("beacon").unwrap(),
            version: ServerVersion::new("0.1.0").unwrap(),
            endpoint: ServerEndpoint::new(
                url::Url::parse("http://127.0.0.1:9/beacon/mcp").unwrap(),
            )
            .unwrap(),
            surfaces: Some(serde_json::from_value(serde_json::json!({"appOrigin": true})).unwrap()),
        },
        authorization,
    )
    .unwrap();
    let issuer = url::Url::parse(&hub.base).unwrap();
    let endpoint = RegistrationClient::discover(&http, &issuer).await.unwrap();
    let registered = RegistrationClient::new(
        http.clone(),
        endpoint,
        secrecy::SecretString::from(hub.secret("VEOVEO_REGISTRATION_SECRET").to_owned()),
        key,
        metadata,
    )
    .unwrap()
    .register()
    .await
    .unwrap();
    assert_eq!(
        registered.app_origin().map(|origin| origin.as_str()),
        Some("https://beacon.apps.veoveo.example")
    );
    assert!(registered.public_base().is_none());
}
