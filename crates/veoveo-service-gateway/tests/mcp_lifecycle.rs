//! Runs the Gateway binary against a disposable PostgreSQL database and checks the MCP
//! `2026-07-28` lifecycle it serves: the versions and resource capabilities it
//! advertises, and its answer to `resources/subscribe`, a method that version replaced
//! with `subscriptions/listen`.

#[path = "support/gateway.rs"]
mod gateway;

use std::time::Duration;

use gateway::{DISCOVER, Gateway, MCP_VERSION, Mcp};
use rmcp::{
    ClientLifecycleMode, ClientServiceExt,
    model::{ErrorCode, ProtocolVersion, SubscriptionFilter},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use veoveo_fixtures::store::TestDb;

#[tokio::test]
async fn resource_subscriptions_are_a_listen_filter() {
    tokio::time::timeout(Duration::from_secs(120), subscriptions())
        .await
        .expect("the subscription lifecycle checks exceeded two minutes");
}

async fn subscriptions() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    let gateway = Gateway::start(directory.path(), &db, Duration::from_secs(60)).await;
    let http = reqwest::Client::new();
    let token = gateway.access_token(&http, gateway.caller("member")).await;
    let bearer = format!("Bearer {token}");

    // MCP 2026-07-28, Resources § Capabilities: `subscribe` declares update
    // notifications for the URIs a `subscriptions/listen` request names in
    // `resourceSubscriptions`.
    let discovered: Value = gateway
        .mcp(&http, Some(&bearer), DISCOVER)
        .await
        .json()
        .await
        .unwrap();
    let result = &discovered["result"];
    assert_eq!(result["supportedVersions"], json!([MCP_VERSION]));
    assert_eq!(result["capabilities"]["resources"]["subscribe"], true);
    assert_eq!(result["capabilities"]["resources"]["listChanged"], true);

    // Subscriptions § Opening a Stream: `subscriptions/listen` replaced
    // `resources/subscribe`, so 2026-07-28 knows neither it nor `resources/unsubscribe`,
    // and JSON-RPC 2.0 §5.1 answers Method not found.
    let subscribe = Mcp {
        method: "resources/subscribe",
        uri: Some("example://resource"),
        version: MCP_VERSION,
    };
    for request in [
        subscribe,
        Mcp {
            method: "resources/unsubscribe",
            ..subscribe
        },
    ] {
        let response = gateway.mcp(&http, Some(&bearer), request).await;
        assert!(response.status().is_client_error(), "{}", request.method);
        let answer: Value = response.json().await.unwrap();
        assert_eq!(answer["id"], 1);
        assert_eq!(
            answer["error"]["code"],
            ErrorCode::METHOD_NOT_FOUND.0,
            "{answer}"
        );
    }
    // A request for a version whose lifecycle still had the method is refused for its
    // version, which the Gateway does not serve.
    let legacy = ProtocolVersion::V_2025_11_25;
    let response = gateway
        .mcp(
            &http,
            Some(&bearer),
            Mcp {
                version: "2025-11-25",
                ..subscribe
            },
        )
        .await;
    let answer: Value = response.json().await.unwrap();
    assert_eq!(
        answer["error"]["code"],
        ErrorCode::UNSUPPORTED_PROTOCOL_VERSION.0,
        "{answer}"
    );
    assert_eq!(answer["error"]["data"]["requested"], json!(legacy));
    assert_eq!(answer["error"]["data"]["supported"], json!([MCP_VERSION]));

    // A listen stream acknowledges the categories it honors. No registered server owns
    // the URI, so the acknowledgement omits it.
    let client = ()
        .serve_with_lifecycle(
            StreamableHttpClientTransport::from_config(
                StreamableHttpClientTransportConfig::with_uri(
                    gateway.installation.resource.clone(),
                )
                .auth_header(token),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .unwrap();
    let mut subscription = client
        .listen(
            SubscriptionFilter::builder()
                .resources_list_changed()
                .resource_subscription("example://resource")
                .build(),
        )
        .await
        .unwrap();
    assert_eq!(
        subscription.acknowledged().resources_list_changed,
        Some(true)
    );
    assert_eq!(subscription.acknowledged().resource_subscriptions, None);
    subscription.cancel().await.unwrap();
    client.cancel().await.unwrap();
    drop(gateway);
}
