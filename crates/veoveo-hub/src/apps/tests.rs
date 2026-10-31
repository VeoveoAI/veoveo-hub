//! The Apps host end to end, in process: a real browser session of a ready Computer,
//! the host page, the bridge and its live-resource listens behind the Hub's request
//! observation, against a fake MCP endpoint of the Computers' profile behind the Hub's
//! own profile authentication. The endpoint serves a catalog of its own: the App,
//! another App of its server, another server's App and an App whose declared policy the
//! Hub refuses, with linked and unlinked tools; and it answers a listen with an event
//! stream of its own messages. Every credential and value is synthetic fixture data.
use std::sync::{
    Arc, Mutex, Once,
    atomic::{AtomicUsize, Ordering},
};

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
use sha2::{Digest, Sha256};
use tower::ServiceExt as _;
use veoveo_hub_sdk::apps::{
    ResourceUiMeta, UiCsp, UiVisibility, app_resource, app_resource_with_meta, link_tool_to_app,
};

use super::{AppOrigins, AppsService};
use crate::{HubCatalog, HubCatalogHandle, HubState};

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
    received: Arc<Mutex<Vec<Received>>>,
    /// The JSON text of every listen message the endpoint sent.
    sent: Arc<Mutex<Vec<String>>>,
    /// How many of the listen streams it opened have closed.
    closes: Arc<AtomicUsize>,
}

/// The Hub's own profile authentication, which admits every request of `/mcp/{profile}`:
/// the bearer's signature, issuer, audience and scope, its invocation authority, its
/// session family's state and revocation, and its authentication record.
fn profile_authentication(
    hub: &HubState,
    catalog: &HubCatalogHandle,
) -> crate::http::ProfileAuthState {
    use base64::Engine as _;
    let issuer = veoveo_hub_sdk::TokenIssuer::parse(HOST).unwrap();
    let key = || {
        veoveo_hub_sdk::HubInternalSigningKey::new(
            base64::engine::general_purpose::STANDARD
                .decode(SIGNING_KEY)
                .unwrap(),
        )
        .unwrap()
    };
    let registrations = crate::registration::RegistrationService::new(
        hub.platform_store().clone(),
        catalog.clone(),
        hub.clone(),
        crate::registration::RegistrationIdentity {
            issuer: issuer.clone(),
            signing_key: key(),
            verification_key: None,
        },
        crate::registration::ServerDatabase {
            url: veoveo_hub_sdk::registration::DatabaseUrl::new(
                url::Url::parse("postgres://127.0.0.1/unused").unwrap(),
            )
            .unwrap(),
        },
        std::time::Duration::from_secs(60),
        Some(AppOrigins::new(HOST, SANDBOX.parse().unwrap()).unwrap()),
    )
    .unwrap();
    let delegation = crate::delegation::DelegationService::new(
        registrations,
        catalog.clone(),
        hub.clone(),
        veoveo_hub_sdk::HubInternalTokenIssuer::new(issuer, key()).unwrap(),
    )
    .unwrap();
    crate::http::ProfileAuthState {
        catalog: catalog.clone(),
        hub_state: hub.clone(),
        deployment: veoveo_hub_sdk::PublicDeployment::new(HOST).unwrap(),
        auth_http: Arc::new(parking_lot::RwLock::new(reqwest::Client::new())),
        delegation,
    }
}

/// Counts the listen streams the endpoint opened that have closed since.
struct Closes(Arc<AtomicUsize>);

impl Drop for Closes {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// The endpoint's answer to a listen: its acknowledgement and one update, each naming
/// the listen, then nothing until the stream closes. A listen of `fixture://ending`
/// ends at once with the listen's final response instead. Every message it sends is
/// recorded as the JSON text it sent.
fn listen_stream(endpoint: &Endpoint, id: &Value, params: &Value) -> Response {
    let named = json!({"io.modelcontextprotocol/subscriptionId": id});
    let uris = params["notifications"]["resourceSubscriptions"].clone();
    let ending = uris == json!(["fixture://ending"]);
    let mut messages = vec![json!({
        "jsonrpc": "2.0",
        "method": "notifications/subscriptions/acknowledged",
        "params": {"_meta": named, "notifications": {"resourceSubscriptions": uris}}
    })];
    messages.push(if ending {
        json!({"jsonrpc": "2.0", "id": id, "result": {"_meta": named}})
    } else {
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/resources/updated",
            "params": {"_meta": named, "uri": uris[0]}
        })
    });
    let messages: Vec<String> = messages.iter().map(Value::to_string).collect();
    endpoint
        .sent
        .lock()
        .unwrap()
        .extend(messages.iter().cloned());
    let closes = Closes(endpoint.closes.clone());
    let body = async_stream::stream! {
        let _closes = closes;
        for message in messages {
            yield Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(format!("data: {message}\n\n")));
        }
        if !ending {
            std::future::pending::<()>().await;
        }
    };
    (
        [(header::CONTENT_TYPE, "text/event-stream")],
        Body::from_stream(body),
    )
        .into_response()
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
    // The Hub's profile authentication admitted the person's bearer of the session.
    let subject = request
        .extensions()
        .get::<crate::AuthenticatedSubject>()
        .cloned()
        .expect("the profile authentication admits the bearer");
    assert!(
        subject.access_token.session_family.is_some(),
        "the bearer names the session"
    );
    assert_eq!(
        subject.access_token.oauth_client_id.as_str(),
        "operator-local-public"
    );
    let bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .unwrap();
    let message: Value = serde_json::from_slice(&bytes).unwrap();
    let method = message["method"].as_str().unwrap().to_owned();
    let params = message["params"].clone();
    let lifetime = subject.access_token.expires_at - subject.access_token.issued_at;
    if method == "subscriptions/listen" {
        assert!(
            lifetime > chrono::TimeDelta::seconds(60) && lifetime <= chrono::TimeDelta::minutes(15),
            "a listen's bearer lives as long as a listen's forwarded token: {lifetime}"
        );
    } else {
        assert!(
            lifetime <= chrono::TimeDelta::seconds(60),
            "a request's bearer lives at most a minute"
        );
    }
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
    if method == "subscriptions/listen" {
        return listen_stream(&endpoint, &message["id"], &params);
    }
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
    catalog: HubCatalogHandle,
    hub: HubState,
    cookie: String,
    received: Arc<Mutex<Vec<Received>>>,
    sent: Arc<Mutex<Vec<String>>>,
    closes: Arc<AtomicUsize>,
}

/// The listens one browser session may hold in these tests, and how soon an open listen
/// notices that its session ended.
const LIMITS: super::ListenLimits = super::ListenLimits {
    per_session: std::num::NonZeroUsize::new(2).unwrap(),
    recheck: std::time::Duration::from_millis(100),
};

impl Fixture {
    async fn start(
        db: &crate::test_store::TestDb,
    ) -> (Self, crate::computers::test_fixture::AppsOwner) {
        let owner = crate::computers::test_fixture::apps_owner(db, catalog()).await;
        let catalog = HubCatalogHandle::new(Arc::new(owner.catalog.clone()));
        let endpoint = Endpoint {
            received: Arc::default(),
            sent: Arc::default(),
            closes: Arc::default(),
        };
        // The fake endpoint stands where the Hub's MCP handler does, behind the Hub's own
        // profile authentication, as `/mcp/{profile}` is.
        let mcp = Router::new()
            .route("/mcp/{profile}", post(mcp))
            .with_state(endpoint.clone())
            .layer(axum::middleware::from_fn_with_state(
                profile_authentication(&owner.hub, &catalog),
                crate::http::authenticate_profile,
            ));
        let router = AppsService::new(
            catalog.clone(),
            owner.hub.clone(),
            AppOrigins::new(HOST, SANDBOX.parse().unwrap()).unwrap(),
            "veoveo.example",
            mcp,
            LIMITS,
        )
        .unwrap()
        .router()
        .layer(axum::middleware::from_fn(
            crate::request_observation::observe_request,
        ));
        (
            Self {
                router,
                catalog,
                hub: owner.hub.clone(),
                cookie: owner.cookie.clone(),
                received: endpoint.received,
                sent: endpoint.sent,
                closes: endpoint.closes,
            },
            owner,
        )
    }

    /// Open a listen of `uris` with the request `id`: its response, whose body is the
    /// listen's stream when it opened.
    async fn listen(&self, id: &str, notifications: Value) -> Response {
        let body = json!({
            "jsonrpc": "2.0", "id": id, "method": "subscriptions/listen",
            "params": {"notifications": notifications}
        });
        self.router
            .clone()
            .oneshot(self.bridge(Some(HOST), body.to_string()))
            .await
            .unwrap()
    }

    /// Wait until `count` of the endpoint's listen streams have closed.
    async fn closed(&self, count: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while self.closes.load(Ordering::SeqCst) < count {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{count} upstream listens closed"));
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
        let script = page
            .split_once("<script src=\"")
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(src, _)| src.to_owned())
            .unwrap();
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

        // The page names its script by the script's digest, which any cache may keep.
        let digest = script
            .strip_prefix("/apps/host-")
            .and_then(|rest| rest.strip_suffix(".js"))
            .unwrap();
        let (status, headers, body) = fixture
            .send(axum::http::Request::get(&script).body(Body::empty()).unwrap())
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "text/javascript; charset=utf-8");
        assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=31536000, immutable");
        let body = String::from_utf8(body).unwrap();
        assert!(body.contains("allow-scripts allow-same-origin"));
        assert_eq!(digest, hex::encode(&Sha256::digest(body.as_bytes())[..16]));
        let stale = axum::http::Request::get("/apps/host.js").body(Body::empty()).unwrap();
        assert_eq!(fixture.send(stale).await.0, StatusCode::NOT_FOUND);
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
        for method in ["tools/list", "resources/list", "ui/initialize", "ui/notifications/resource-updated"] {
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

/// The data of the stream's next event, past its comments; None once it ends.
async fn next_event(events: &mut sse_stream::SseStream<Body>) -> Option<String> {
    use futures::StreamExt as _;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match events.next().await {
                Some(Ok(event)) => {
                    if let Some(data) = event.data {
                        return Some(data);
                    }
                }
                Some(Err(error)) => panic!("the listen's stream broke: {error}"),
                None => return None,
            }
        }
    })
    .await
    .expect("the listen's next event")
}

/// The listen's open stream, from its response.
fn stream(response: Response) -> sse_stream::SseStream<Body> {
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "private, no-store"
    );
    sse_stream::SseStream::new(response.into_body())
}

#[tokio::test]
async fn a_listen_relays_its_servers_own_messages_unchanged_and_ends_with_the_view() {
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let db = crate::test_store::TestDb::new().await;
        let (fixture, _owner) = Fixture::start(&db).await;

        // The acknowledgement and an update reach the page as the server sent them.
        let uri = "fixture://events?cursor=eyJhZnRlciI6MX0%3D";
        let notifications = json!({"resourceSubscriptions": [uri]});
        let mut events = stream(fixture.listen("listen-1", notifications.clone()).await);
        let sent = fixture.sent.lock().unwrap().clone();
        assert_eq!(
            next_event(&mut events).await.as_deref(),
            Some(sent[0].as_str())
        );
        assert_eq!(
            next_event(&mut events).await.as_deref(),
            Some(sent[1].as_str())
        );
        let update: Value = serde_json::from_str(&sent[1]).unwrap();
        assert_eq!(update["method"], "notifications/resources/updated");
        assert_eq!(update["params"]["uri"], uri);
        assert_eq!(
            update["params"]["_meta"]["io.modelcontextprotocol/subscriptionId"], "listen-1",
            "the in-process listen carries the view's own id"
        );
        let listen = fixture
            .received()
            .into_iter()
            .find(|received| received.method == "subscriptions/listen")
            .unwrap();
        assert_eq!(listen.params["notifications"], notifications);

        // The page aborting its fetch ends the listen at the endpoint.
        drop(events);
        fixture.closed(1).await;

        // A listen the endpoint ends delivers its final response and ends with it.
        let mut events = stream(
            fixture
                .listen(
                    "listen-2",
                    json!({"resourceSubscriptions": ["fixture://ending"]}),
                )
                .await,
        );
        assert!(
            next_event(&mut events)
                .await
                .unwrap()
                .contains("notifications/subscriptions/acknowledged")
        );
        let last: Value = serde_json::from_str(&next_event(&mut events).await.unwrap()).unwrap();
        assert_eq!(last["id"], "listen-2");
        assert!(last["result"].is_object());
        assert_eq!(
            next_event(&mut events).await,
            None,
            "nothing follows the final response"
        );
        fixture.closed(2).await;

        // Another server's resource, list changes or Tasks reach no server.
        fixture.received();
        for notifications in [
            json!({"resourceSubscriptions": ["fixture://events", "neighbor://events"]}),
            json!({"resourceSubscriptions": ["ui://neighbor/board.html"]}),
            json!({"resourceSubscriptions": ["fixture://events"], "resourcesListChanged": true}),
            json!({"taskIds": ["task-1"]}),
            json!({"resourceSubscriptions": []}),
        ] {
            let response = fixture.listen("listen-3", notifications.clone()).await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let answer: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(answer["id"], "listen-3");
            assert_eq!(answer["error"]["code"], -32602, "{notifications}");
            assert!(
                fixture
                    .received()
                    .iter()
                    .all(|received| received.method != "subscriptions/listen"),
                "{notifications} reached no server"
            );
        }
    })
    .await
    .expect("App listen deadline");
}

#[tokio::test]
async fn a_session_holds_its_ceiling_of_listens_and_its_end_closes_them() {
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let db = crate::test_store::TestDb::new().await;
        let (fixture, owner) = Fixture::start(&db).await;
        let notifications = || json!({"resourceSubscriptions": ["fixture://events"]});

        // The session's ceiling refuses one listen more, until one ends.
        let first = stream(fixture.listen("first", notifications()).await);
        let mut second = stream(fixture.listen("second", notifications()).await);
        let refused = fixture.listen("third", notifications()).await;
        assert_eq!(refused.status(), StatusCode::TOO_MANY_REQUESTS);
        let listens = fixture
            .received()
            .into_iter()
            .filter(|received| received.method == "subscriptions/listen")
            .count();
        assert_eq!(listens, 2, "the refused listen reached no server");
        drop(first);
        fixture.closed(1).await;
        let mut third = stream(fixture.listen("third", notifications()).await);

        // The session's end closes every open listen, each with an error naming it.
        for events in [&mut second, &mut third] {
            assert!(next_event(events).await.unwrap().contains("acknowledged"));
            assert!(next_event(events).await.unwrap().contains("resources/updated"));
        }
        owner.revoke().await;
        for (id, events) in [("second", &mut second), ("third", &mut third)] {
            let ended: Value = serde_json::from_str(&next_event(events).await.unwrap()).unwrap();
            assert_eq!(
                ended,
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32603, "message": "The browser session ended; sign in to listen again."}
                })
            );
            assert_eq!(next_event(events).await, None);
        }
        fixture.closed(3).await;
        assert_eq!(
            fixture.listen("fourth", notifications()).await.status(),
            StatusCode::UNAUTHORIZED
        );
    })
    .await
    .expect("App listen ceiling deadline");
}

/// What a server behind its app origin saw of each request.
#[derive(Debug, Clone)]
struct OriginSeen {
    path: String,
    host: String,
    authorization: Option<String>,
    cookie: Option<String>,
}

/// A server that records what reaches it and tries to set the Hub's cookie beside its own.
async fn origin_server() -> (String, Arc<Mutex<Vec<OriginSeen>>>) {
    let seen: Arc<Mutex<Vec<OriginSeen>>> = Arc::default();
    let recorded = seen.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().fallback(move |request: Request| {
        let recorded = recorded.clone();
        async move {
            let header = |name| {
                request
                    .headers()
                    .get(name)
                    .and_then(|value: &axum::http::HeaderValue| value.to_str().ok())
                    .map(str::to_owned)
            };
            recorded.lock().unwrap().push(OriginSeen {
                path: request.uri().to_string(),
                host: header(header::HOST).unwrap_or_default(),
                authorization: header(header::AUTHORIZATION),
                cookie: header(header::COOKIE),
            });
            (
                [
                    (header::SET_COOKIE, "__Host-veoveo-app=stolen; Path=/"),
                    (header::SET_COOKIE, "editor=own; Path=/"),
                ],
                "served by the app",
            )
        }
    });
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}/beacon/mcp"), seen)
}

/// The value of `name` among a response's `Set-Cookie` fields.
fn set_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| {
            value
                .split(';')
                .next()
                .and_then(|pair| pair.split_once('='))
                .filter(|(key, _)| *key == name)
                .map(|(_, value)| value.to_owned())
        })
}

#[tokio::test]
async fn an_app_origin_signs_the_person_in_through_the_issuer_and_forwards_as_them() {
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        use veoveo_hub_sdk::registration::{
            AuthorizationDeclaration, Requirements, RouteAccess, SurfacesDeclaration,
        };
        let db = crate::test_store::TestDb::new().await;
        let (fixture, _owner) = Fixture::start(&db).await;
        let (endpoint, seen) = origin_server().await;
        let register = |access: RouteAccess| {
            let mut server = crate::test_catalog::registered_server("beacon", &endpoint);
            server.surfaces = Some(SurfacesDeclaration {
                public_routes: Vec::new(),
                app_origin: true,
            });
            server.authorization = Arc::new(AuthorizationDeclaration {
                permissions: Vec::new(),
                requirements: Requirements {
                    app_origin: Some(access),
                    ..Default::default()
                },
                uses: Vec::new(),
                calls_hub: false,
            });
            fixture
                .catalog
                .replace_registrations(crate::registration::RegistrationSnapshot::new([server]));
        };
        register(RouteAccess::Authenticated);
        let origins = AppOrigins::new(HOST, SANDBOX.parse().unwrap()).unwrap();
        let issuer = veoveo_hub_sdk::HubInternalTokenIssuer::new(
            veoveo_hub_sdk::TokenIssuer::parse(HOST).unwrap(),
            veoveo_hub_sdk::HubInternalSigningKey::new(
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, SIGNING_KEY)
                    .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let servers = super::ServerOrigins::new(
            fixture.catalog.clone(),
            fixture.hub.clone(),
            origins.clone(),
            profile_authentication(&fixture.hub, &fixture.catalog),
            issuer,
            crate::HubUpstreamHttpClientPool::new(),
        );
        // The entry is on the issuer's listener, the origins on the Apps listener.
        let entry = servers
            .clone()
            .entry_router()
            .layer(axum::middleware::from_fn(
                crate::request_observation::observe_request,
            ));
        let listener = super::SandboxProxy::new(origins, Some(servers))
            .router()
            .layer(axum::middleware::from_fn(
                crate::request_observation::observe_request,
            ));
        let enter = |path: &str, signed_in: bool| {
            let entry = entry.clone();
            let request = fixture.open(path, signed_in);
            async move {
                let response = entry.oneshot(request).await.unwrap();
                (response.status(), response.headers().clone())
            }
        };
        let on_origin = |path: &str, cookies: &[(&str, &str)], navigate: bool| {
            let mut request = axum::http::Request::get(path)
                .header("x-forwarded-host", "beacon.apps.veoveo.example");
            if navigate {
                request = request
                    .header("sec-fetch-mode", "navigate")
                    .header("sec-fetch-dest", "document");
            }
            if !cookies.is_empty() {
                let cookie: Vec<String> = cookies.iter().map(|(k, v)| format!("{k}={v}")).collect();
                request = request.header(header::COOKIE, cookie.join("; "));
            }
            request.body(Body::empty()).unwrap()
        };
        let send = |request: axum::http::Request<Body>| {
            let listener = listener.clone();
            async move {
                let response = listener.oneshot(request).await.unwrap();
                let status = response.status();
                let headers = response.headers().clone();
                let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                (status, headers, String::from_utf8_lossy(&body).into_owned())
            }
        };

        // Without a session, a page load goes to the issuer with new state; a fetch is
        // refused; the sandbox origin and an origin no server declares stay the proxy's.
        let (status, headers, _) = send(on_origin("/editor?id=7", &[], true)).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let state = set_cookie(&headers, "__Host-veoveo-app-state").unwrap();
        let entry = headers[header::LOCATION].to_str().unwrap().to_owned();
        let expected = format!(
            "{HOST}/app-origins/beacon?path=%2Feditor%3Fid%3D7&state={}",
            urlencoding(&super::server_origin::tests_digest(&state))
        );
        assert_eq!(entry, expected);
        let (status, _, _) = send(on_origin("/editor", &[], false)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        for host in [
            "sandbox.apps.veoveo.example",
            "elsewhere.apps.veoveo.example",
        ] {
            let request = axum::http::Request::get("/editor")
                .header("x-forwarded-host", host)
                .body(Body::empty())
                .unwrap();
            assert_eq!(send(request).await.0, StatusCode::NOT_FOUND, "{host}");
        }

        // On the issuer, the signed-in person gets a code for that browser's state.
        let entry_path = entry.trim_start_matches(HOST);
        let (status, headers) = enter(entry_path, true).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let redeem = headers[header::LOCATION].to_str().unwrap().to_owned();
        let session_path = redeem
            .strip_prefix("https://beacon.apps.veoveo.example")
            .unwrap()
            .to_owned();
        assert!(session_path.starts_with("/_veoveo/session?code="));
        // Signed out, the entry signs in first and comes back to itself.
        let (status, headers) = enter(entry_path, false).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(headers[header::LOCATION], "/computer");

        // Another browser, without the state, cannot redeem the code; this one can, once.
        let other = "0".repeat(64);
        let (status, _, _) = send(on_origin(
            &session_path,
            &[("__Host-veoveo-app-state", &other)],
            true,
        ))
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, headers, _) = send(on_origin(
            &session_path,
            &[("__Host-veoveo-app-state", &state)],
            true,
        ))
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(headers[header::LOCATION], "/editor?id=7");
        let session = set_cookie(&headers, "__Host-veoveo-app").unwrap();
        assert_eq!(
            set_cookie(&headers, "__Host-veoveo-app-state").as_deref(),
            Some("")
        );
        let (status, _, _) = send(on_origin(
            &session_path,
            &[("__Host-veoveo-app-state", &state)],
            true,
        ))
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "a code is redeemed once");

        // With the session, requests reach the server as the person, with a forwarded
        // token, the app origin's host and the application's own cookies only.
        let (status, headers, body) = send(on_origin(
            "/editor?id=7",
            &[("__Host-veoveo-app", &session), ("editor", "own")],
            false,
        ))
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body, "served by the app");
        let set: Vec<_> = headers.get_all(header::SET_COOKIE).iter().collect();
        assert_eq!(set, ["editor=own; Path=/"], "the app sets no Hub cookie");
        let request = seen.lock().unwrap().pop().unwrap();
        assert_eq!(request.path, "/editor?id=7");
        assert_eq!(request.host, "beacon.apps.veoveo.example");
        assert_eq!(request.cookie.as_deref(), Some("editor=own"));
        let bearer = request.authorization.unwrap();
        let token = bearer.strip_prefix("Bearer ").unwrap();
        let claims: Value = serde_json::from_slice(
            &base64::Engine::decode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                token.split('.').nth(1).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(claims["aud"], endpoint.as_str());
        assert!(claims["exp"].as_i64().unwrap() - claims["iat"].as_i64().unwrap() <= 60);

        // A session for one app origin opens no other.
        let request = axum::http::Request::get("/editor")
            .header("x-forwarded-host", "neighbor.apps.veoveo.example")
            .header(header::COOKIE, format!("__Host-veoveo-app={session}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(request).await.0, StatusCode::NOT_FOUND);

        // Once the server's requirement is a permission the person lacks, both the entry
        // and every request are refused.
        register(RouteAccess::Permission(
            veoveo_types::PermissionName::parse("beacon.editor.use").unwrap(),
        ));
        let (status, _) = enter(entry_path, true).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _, _) = send(on_origin(
            "/editor",
            &[("__Host-veoveo-app", &session)],
            false,
        ))
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(
            seen.lock().unwrap().is_empty(),
            "nothing refused reaches the server"
        );
    })
    .await
    .expect("app origin deadline");
}

fn urlencoding(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
