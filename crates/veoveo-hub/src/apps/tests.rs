//! The Apps host end to end, in process: a real browser session of a ready Computer,
//! the host page and the bridge, against a fake MCP endpoint of the Computers' profile
//! that verifies each request's bearer as the Hub's profile admission does and serves a
//! catalog of its own: the App, another App of its server, another server's App and an
//! App whose declared policy the Hub refuses, with linked and unlinked tools. Every
//! credential and value is synthetic fixture data.
use std::sync::{Arc, Mutex, Once};

use axum::{
    Json, Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use rmcp::model::{JsonObject, Resource, Tool};
use serde_json::{Value, json};
use tower::ServiceExt as _;
use veoveo_hub_sdk::apps::{
    ResourceUiMeta, UiCsp, UiVisibility, app_resource, app_resource_with_meta, link_tool_to_app,
};

use super::{AppOrigins, AppsService};
use crate::{BearerToken, HubCatalog, HubCatalogHandle, HubState, JwtAuthConfig, JwtVerifier};

/// A synthetic Ed25519 PKCS#8 key the test catalog's authorization server signs with.
const SIGNING_KEY: &str = "MC4CAQAwBQYDK2VwBCIEII4AsVspz8h7mpqvOkgslJP07HfqpiWMZA+6Ii90lVBl";
const SIGNING_KEY_ENV: &str = "VEOVEO_APPS_TEST_SIGNING_KEY_B64";
const HOST: &str = "https://veoveo.example";
const SANDBOX: &str = "https://sandbox.apps.veoveo.example";
const BOARD: &str = "/apps/fixture/board.html";
/// The largest integer Time reports, which the bridge answers exactly.
const LARGEST: u64 = u64::MAX;

/// The test catalog with an authorization server that signs with [`SIGNING_KEY`].
fn catalog() -> HubCatalog {
    static KEY: Once = Once::new();
    // SAFETY: nextest runs each test in its own process, and this sets one variable
    // once, to one value, before any thread of the test reads it.
    KEY.call_once(|| unsafe { std::env::set_var(SIGNING_KEY_ENV, SIGNING_KEY) });
    let key = crate::http::auth_support::AccessTokenSigningKey::from_base64(SIGNING_KEY).unwrap();
    let mut plane: Value = serde_json::from_str(include_str!("../../testdata/hub.json")).unwrap();
    plane["authorizationServers"][0]["accessTokenKeyId"] = json!(key.key_id());
    for secret in plane["secrets"].as_array_mut().unwrap() {
        if secret["id"] == "veoveo_access_token_private_key" {
            secret["locator"] = json!(SIGNING_KEY_ENV);
        }
    }
    HubCatalog::from_control_plane(
        serde_json::from_value(plane).unwrap(),
        crate::catalog_fixture::binding(),
    )
    .unwrap()
}

/// The catalog the fake endpoint serves, in two resource pages.
fn resources() -> (Vec<Resource>, Vec<Resource>) {
    let board = app_resource_with_meta(
        "ui://fixture/board.html",
        "board",
        ResourceUiMeta {
            csp: Some(UiCsp {
                connect_domains: vec!["https://api.example.net".into()],
                frame_domains: vec!["https://fixture.apps.veoveo.example".into()],
                ..UiCsp::default()
            }),
            prefers_border: None,
        },
    )
    .with_title("Board </script>");
    let framing = app_resource_with_meta(
        "ui://fixture/framing.html",
        "framing",
        ResourceUiMeta {
            csp: Some(UiCsp {
                frame_domains: vec!["https://neighbor.apps.veoveo.example".into()],
                ..UiCsp::default()
            }),
            prefers_border: None,
        },
    );
    let mut plain = Resource::new("ui://fixture/plain.html", "plain");
    plain.mime_type = Some("text/html".into());
    (
        vec![board, app_resource("ui://neighbor/board.html", "board")],
        vec![
            app_resource("ui://fixture/other.html", "other"),
            framing,
            plain,
        ],
    )
}

fn tools() -> Vec<Tool> {
    let tool = |name: &str| {
        Tool::new(
            name.to_owned(),
            "fixture tool",
            serde_json::from_value::<JsonObject>(json!({"type": "object"})).unwrap(),
        )
    };
    let board = "ui://fixture/board.html";
    vec![
        link_tool_to_app(tool("fixture__move"), board, &[UiVisibility::App]),
        link_tool_to_app(tool("fixture__summarize"), board, &[UiVisibility::Model]),
        link_tool_to_app(tool("fixture__other"), "ui://fixture/other.html", &[]),
        tool("fixture__unlinked"),
        link_tool_to_app(tool("neighbor__play"), "ui://neighbor/board.html", &[]),
    ]
}

/// What the fake endpoint received, as it received it.
#[derive(Debug, Clone)]
struct Received {
    method: String,
    name: Option<String>,
    params: Value,
}

#[derive(Clone)]
struct Endpoint {
    hub: HubState,
    catalog: Arc<HubCatalog>,
    received: Arc<Mutex<Vec<Received>>>,
}

/// The bearer admitted as the Hub's profile authentication admits it: signature,
/// issuer, audience, scope, invocation authority and the session family's state.
async fn admitted(endpoint: &Endpoint, headers: &HeaderMap) -> bool {
    let catalog = &endpoint.catalog;
    let profile = catalog.profile(&"operator".parse().unwrap()).unwrap();
    let server = catalog
        .authorization_server(&profile.authorization_server)
        .unwrap();
    let Some(Ok(value)) = headers
        .get(header::AUTHORIZATION)
        .map(|value| value.to_str())
    else {
        return false;
    };
    let Ok(bearer) = BearerToken::from_authorization_header(value) else {
        return false;
    };
    let jwks = crate::issuance::authorization_server_jwks_from_signing_key(catalog, server)
        .await
        .unwrap();
    let config = JwtAuthConfig::new(
        server.issuer.clone(),
        profile.protected_resource.clone(),
        profile.required_scopes.iter().cloned().collect(),
        crate::http::auth_support::allowed_hub_jwt_algorithms(),
    )
    .unwrap();
    let Ok(verified) = JwtVerifier::new(config, jwks)
        .with_extensions(endpoint.hub.token_extension_registry().unwrap())
        .unwrap()
        .verify(&bearer)
    else {
        return false;
    };
    let subject = endpoint
        .hub
        .resolve_authenticated_subject(catalog, verified)
        .await
        .unwrap();
    assert!(
        subject.access_token.session_family.is_some(),
        "the bearer names the session"
    );
    assert!(
        subject.access_token.expires_at - subject.access_token.issued_at
            <= chrono::TimeDelta::seconds(60),
        "the bearer lives at most a minute"
    );
    assert_eq!(
        subject.access_token.oauth_client_id.as_str(),
        "operator-local-public"
    );
    endpoint
        .hub
        .access_token_session_valid(
            &profile.protected_resource,
            &server.id,
            &subject.access_token,
            &subject.principal,
        )
        .await
        .unwrap()
}

async fn mcp(State(endpoint): State<Endpoint>, request: Request) -> Response {
    let headers = request.headers().clone();
    // The Hub's MCP endpoint refuses a request without the browser request's correlation.
    assert!(
        request
            .extensions()
            .get::<crate::request_observation::RequestObservation>()
            .is_some(),
        "the in-process request carries the browser request's correlation"
    );
    if !admitted(&endpoint, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .unwrap();
    let message: Value = serde_json::from_slice(&bytes).unwrap();
    let method = message["method"].as_str().unwrap().to_owned();
    let params = message["params"].clone();
    assert_eq!(headers["mcp-protocol-version"], "2026-07-28");
    assert_eq!(headers["mcp-method"], method.as_str());
    let meta = &params["_meta"];
    assert_eq!(
        meta["io.modelcontextprotocol/protocolVersion"],
        "2026-07-28"
    );
    assert_eq!(
        meta["io.modelcontextprotocol/clientInfo"]["name"],
        "veoveo-hub-apps"
    );
    let extensions = &meta["io.modelcontextprotocol/clientCapabilities"]["extensions"];
    assert!(
        extensions["io.modelcontextprotocol/ui"].is_object(),
        "Apps traffic declares the Apps extension"
    );
    assert!(
        extensions["io.modelcontextprotocol/tasks"].is_object(),
        "Apps traffic declares Tasks"
    );
    endpoint.received.lock().unwrap().push(Received {
        method: method.clone(),
        name: headers
            .get("mcp-name")
            .map(|value| value.to_str().unwrap().to_owned()),
        params: params.clone(),
    });
    let (first, second) = resources();
    let answer = match method.as_str() {
        "resources/list" if params.get("cursor").is_none() => {
            json!({"result": {"resources": first, "nextCursor": "second"}})
        }
        "resources/list" => json!({"result": {"resources": second}}),
        "tools/list" => json!({"result": {"tools": tools()}}),
        "tools/call" => json!({"result": {
            "content": [{"type": "text", "text": "moved"}],
            "structuredContent": {"called": params["name"], "largest": LARGEST}
        }}),
        "resources/read" => json!({"result": {"contents": [{
            "uri": params["uri"], "mimeType": "text/html;profile=mcp-app", "text": "<!doctype html><p>board</p>"
        }]}}),
        "tasks/get" => {
            json!({"error": {"code": -32602, "message": "unknown Task", "data": {"taskId": params["taskId"]}}})
        }
        _ => json!({"error": {"code": -32601, "message": "unknown method"}}),
    };
    let mut answer = answer.as_object().unwrap().clone();
    answer.insert("jsonrpc".into(), json!("2.0"));
    answer.insert("id".into(), message["id"].clone());
    Json(Value::Object(answer)).into_response()
}

struct Fixture {
    router: Router,
    cookie: String,
    received: Arc<Mutex<Vec<Received>>>,
}

impl Fixture {
    async fn start(
        db: &crate::test_store::TestDb,
    ) -> (Self, crate::computers::test_fixture::AppsOwner) {
        let owner = crate::computers::test_fixture::apps_owner(db, catalog()).await;
        let catalog = Arc::new(owner.catalog.clone());
        let received = Arc::new(Mutex::new(Vec::new()));
        let endpoint = Router::new()
            .route("/mcp/{profile}", post(mcp))
            .with_state(Endpoint {
                hub: owner.hub.clone(),
                catalog: catalog.clone(),
                received: received.clone(),
            });
        let router = AppsService::new(
            HubCatalogHandle::new(catalog.clone()),
            owner.hub.clone(),
            AppOrigins::new(HOST, SANDBOX.parse().unwrap()).unwrap(),
            "veoveo.example",
            endpoint,
        )
        .unwrap()
        .router()
        .layer(axum::middleware::from_fn(
            crate::request_observation::observe_request,
        ));
        (
            Self {
                router,
                cookie: owner.cookie.clone(),
                received,
            },
            owner,
        )
    }

    async fn send(&self, request: axum::http::Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, headers, body.to_vec())
    }

    fn open(&self, path: &str, signed_in: bool) -> axum::http::Request<Body> {
        let mut request = axum::http::Request::get(path)
            .header("sec-fetch-mode", "navigate")
            .header("sec-fetch-dest", "document");
        if signed_in {
            request = request.header(header::COOKIE, format!("__Host-computer={}", self.cookie));
        }
        request.body(Body::empty()).unwrap()
    }

    fn bridge(&self, origin: Option<&str>, body: impl Into<Body>) -> axum::http::Request<Body> {
        let mut request = axum::http::Request::post(BOARD)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, format!("__Host-computer={}", self.cookie));
        if let Some(origin) = origin {
            request = request.header(header::ORIGIN, origin);
        }
        request.body(body.into()).unwrap()
    }

    /// One view request through the bridge; its status and JSON-RPC response text.
    async fn call(&self, method: &str, params: Value) -> (StatusCode, String) {
        let body = json!({"jsonrpc": "2.0", "id": "view-7", "method": method, "params": params});
        let (status, _, bytes) = self.send(self.bridge(Some(HOST), body.to_string())).await;
        (status, String::from_utf8(bytes).unwrap())
    }

    /// The methods the endpoint received since the last call, and clears them.
    fn received(&self) -> Vec<Received> {
        std::mem::take(&mut *self.received.lock().unwrap())
    }
}

fn response(text: &str) -> Value {
    let response: Value = serde_json::from_str(text).unwrap();
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], "view-7", "the view's own id");
    response
}

#[tokio::test]
async fn the_host_opens_only_an_app_the_person_sees_and_signs_in_first() {
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let db = crate::test_store::TestDb::new().await;
        let (fixture, _owner) = Fixture::start(&db).await;

        let (status, headers, body) = fixture.send(fixture.open(BOARD, true)).await;
        let page = String::from_utf8(body).unwrap();
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(
            headers[header::CONTENT_SECURITY_POLICY],
            "default-src 'none'; script-src 'self'; connect-src 'self'; \
             frame-src https://sandbox.apps.veoveo.example; img-src 'self' data:; base-uri 'none'; \
             form-action 'none'; frame-ancestors 'none'"
        );
        assert_eq!(headers[header::CACHE_CONTROL], "private, no-store");
        let data = page
            .split_once("<script type=\"application/json\" id=\"app\">")
            .and_then(|(_, rest)| rest.split_once("</script>"))
            .map(|(data, _)| data)
            .unwrap();
        let data: Value = serde_json::from_str(data).unwrap();
        assert_eq!(data["uri"], "ui://fixture/board.html");
        assert_eq!(data["title"], "Board </script>", "the title is data, never markup");
        assert_eq!(data["sandbox"], SANDBOX);
        assert_eq!(
            data["proxy"],
            "https://sandbox.apps.veoveo.example/?connect=https%3A%2F%2Fapi.example.net&frame=https%3A%2F%2Ffixture.apps.veoveo.example"
        );
        assert_eq!(data["csp"]["connectDomains"], json!(["https://api.example.net"]));
        assert!(page.contains("<title>Board &lt;/script&gt;</title>"));
        assert_eq!(fixture.received().len(), 2, "both resource pages were read");

        // Signed out, a page load signs in and comes back; anything else is refused.
        let (status, headers, _) = fixture.send(fixture.open(BOARD, false)).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(headers[header::LOCATION], "/computer");
        assert_eq!(
            headers[header::SET_COOKIE],
            "__Host-computer-return=/apps/fixture/board.html; Path=/; Max-Age=600; Secure; HttpOnly; SameSite=Lax"
        );
        let fetch = axum::http::Request::get(BOARD).body(Body::empty()).unwrap();
        assert_eq!(fixture.send(fetch).await.0, StatusCode::UNAUTHORIZED);

        for (path, status) in [
            ("/apps/fixture/absent.html", StatusCode::NOT_FOUND),
            ("/apps/fixture/plain.html", StatusCode::NOT_FOUND),
            ("/apps/sandbox/board.html", StatusCode::NOT_FOUND),
            ("/apps/fixture/%62oard.html", StatusCode::NOT_FOUND),
            ("/apps/fixture/framing.html", StatusCode::BAD_GATEWAY),
        ] {
            let (got, _, body) = fixture.send(fixture.open(path, true)).await;
            assert_eq!(got, status, "{path}: {}", String::from_utf8_lossy(&body));
        }
        let (_, _, body) = fixture.send(fixture.open("/apps/fixture/framing.html", true)).await;
        assert!(String::from_utf8(body).unwrap().contains("an App frames only its own server&#39;s app origin"));

        let (status, headers, body) = fixture
            .send(axum::http::Request::get("/apps/host.js").body(Body::empty()).unwrap())
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "text/javascript; charset=utf-8");
        assert!(String::from_utf8(body).unwrap().contains("allow-scripts allow-same-origin"));
    })
    .await
    .expect("App host page deadline");
}

#[tokio::test]
async fn the_bridge_reaches_only_the_apps_own_linked_tools_and_resources_as_the_person() {
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let db = crate::test_store::TestDb::new().await;
        let (fixture, owner) = Fixture::start(&db).await;

        // A linked tool, by the server's own name, is called by its catalog name.
        let (status, text) = fixture.call("tools/call", json!({"name": "move", "arguments": {"to": 3}})).await;
        assert_eq!(status, StatusCode::OK);
        let answer = response(&text);
        assert_eq!(answer["result"]["structuredContent"]["called"], "fixture__move");
        assert!(text.contains("\"largest\":18446744073709551615"), "integers reach the host exactly: {text}");
        let call = fixture.received().into_iter().find(|r| r.method == "tools/call").unwrap();
        assert_eq!(call.name.as_deref(), Some("fixture__move"));
        assert_eq!(call.params["arguments"], json!({"to": 3}));

        // Unlinked, model-only, another App's and another server's tools never leave
        // the Hub.
        for name in ["unlinked", "summarize", "other", "play", "neighbor__play", "fixture__move"] {
            let (status, text) = fixture.call("tools/call", json!({"name": name})).await;
            assert_eq!(status, StatusCode::OK);
            let answer = response(&text);
            assert_eq!(answer["error"]["code"], -32602, "{name}");
            assert!(answer["error"]["message"].as_str().unwrap().contains("is not linked to the App"));
            assert!(fixture.received().iter().all(|r| r.method != "tools/call"), "{name} reached no server");
        }

        // The App's own server's resources travel unchanged, queries included.
        for uri in ["fixture://events?cursor=eyJhZnRlciI6MX0%3D", "ui://fixture/board.html"] {
            let (_, text) = fixture.call("resources/read", json!({"uri": uri})).await;
            assert_eq!(response(&text)["result"]["contents"][0]["uri"], uri);
            let read = fixture.received().into_iter().find(|r| r.method == "resources/read").unwrap();
            assert_eq!(read.params["uri"], uri);
            assert_eq!(read.name.as_deref(), Some(uri));
        }
        for uri in ["neighbor://items/1", "ui://neighbor/board.html"] {
            let (_, text) = fixture.call("resources/read", json!({"uri": uri})).await;
            assert_eq!(response(&text)["error"]["code"], -32602, "{uri}");
            assert!(fixture.received().iter().all(|r| r.method != "resources/read"), "{uri}");
        }

        // A Task request is the person's own; its error passes unchanged.
        let (_, text) = fixture.call("tasks/get", json!({"taskId": "task-1"})).await;
        assert_eq!(
            response(&text)["error"],
            json!({"code": -32602, "message": "unknown Task", "data": {"taskId": "task-1"}})
        );
        assert_eq!(fixture.received().last().unwrap().name.as_deref(), Some("task-1"));

        // Methods the bridge does not serve.
        for method in ["subscriptions/listen", "tools/list", "resources/list", "ui/initialize"] {
            let (_, text) = fixture.call(method, json!({})).await;
            assert_eq!(response(&text)["error"]["code"], -32601, "{method}");
        }
        fixture.received();
        let (_, text) = fixture.call("ping", json!({})).await;
        assert_eq!(response(&text)["result"], json!({}));

        // Only the host page's origin, with a session, sends one bounded request.
        let body = || json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}).to_string();
        assert_eq!(fixture.send(fixture.bridge(None, body())).await.0, StatusCode::FORBIDDEN);
        assert_eq!(fixture.send(fixture.bridge(Some(SANDBOX), body())).await.0, StatusCode::FORBIDDEN);
        assert_eq!(fixture.send(fixture.bridge(Some("null"), body())).await.0, StatusCode::FORBIDDEN);
        let anonymous = axum::http::Request::post(BOARD)
            .header(header::ORIGIN, HOST)
            .body(Body::from(body()))
            .unwrap();
        assert_eq!(fixture.send(anonymous).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(fixture.send(fixture.bridge(Some(HOST), "[]")).await.0, StatusCode::BAD_REQUEST);
        let large = json!({"jsonrpc": "2.0", "id": 1, "method": "ping", "params": {"pad": "x".repeat(300 * 1024)}});
        assert_eq!(
            fixture.send(fixture.bridge(Some(HOST), large.to_string())).await.0,
            StatusCode::PAYLOAD_TOO_LARGE
        );

        // A revoked session no longer authenticates the bridge or opens the App.
        owner.revoke().await;
        assert_eq!(fixture.call("ping", json!({})).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(fixture.send(fixture.open(BOARD, true)).await.0, StatusCode::SEE_OTHER);
    })
    .await
    .expect("App bridge deadline");
}
