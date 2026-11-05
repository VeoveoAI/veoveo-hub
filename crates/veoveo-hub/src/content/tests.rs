//! The content domain end to end, in process: a real browser session of a ready Computer, the
//! Hub's page and the Apps listener's token origins, over a fixture owner and a fixture server
//! that declares a viewer. Every credential and value is synthetic fixture data.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures::future::BoxFuture;
use tower::ServiceExt as _;
use veoveo_hub_sdk::registration::{
    BodyLimit, ContentDeclaration, FileExtension, PublicRouteDeclaration, PublicRoutePrefix,
    RequestRate, SurfacesDeclaration, ViewerDeclaration,
};

use super::{ContentDomain, ContentOrigin, ContentOwner, Named, token, view_return_path};
use crate::{
    HubCatalog, HubCatalogHandle,
    apps::{AppOrigins, SandboxProxy, ServerOrigins},
    computers::{
        BrowserSession,
        test_fixture::{AppsOwner, apps_owner},
    },
    problem::Problem,
    surfaces::PublicRoutes,
    test_store::TestDb,
};

const HOST: &str = "https://veoveo.example";
const SANDBOX: &str = "https://sandbox.apps.veoveo.example";
const CONTENT: &str = "https://veoveo-content.example";
const FRAMING: &str = "frame-ancestors https://veoveo.example 'self'";
/// The fixture server's link, 160 bits in hex.
const LINK_TOKEN: &str = "abcdefabcdefabcdefabcdefabcdefabcdefabcd";

/// An owner of one item, `box`, holding three files, with a switch for the person's access.
struct Fixture {
    allowed: Arc<AtomicBool>,
    reads: Arc<Mutex<Vec<String>>>,
}

impl ContentOwner for Fixture {
    fn named(&self, rest: &str) -> Option<Named> {
        let (item, path) = rest.split_once('/')?;
        (item == "box").then(|| Named {
            item: item.to_owned(),
            path: path.to_owned(),
        })
    }

    fn admit<'a>(
        &'a self,
        _catalog: &'a HubCatalog,
        _session: &'a BrowserSession,
        _item: &'a str,
    ) -> BoxFuture<'a, Result<(), Problem>> {
        Box::pin(async move {
            if self.allowed.load(Ordering::SeqCst) {
                Ok(())
            } else {
                Err(Problem::new(
                    StatusCode::FORBIDDEN,
                    "fixture-refused",
                    "Refused",
                ))
            }
        })
    }

    fn read<'a>(
        &'a self,
        catalog: &'a HubCatalog,
        session: &'a BrowserSession,
        item: &'a str,
        path: &'a str,
        _method: &'a Method,
        headers: &'a HeaderMap,
    ) -> BoxFuture<'a, Result<Response, Problem>> {
        Box::pin(async move {
            self.admit(catalog, session, item).await?;
            assert_eq!(item, "box");
            assert!(headers.get(header::COOKIE).is_none());
            self.reads.lock().unwrap().push(path.to_owned());
            match path {
                "site/page.html" | "a%20b.png" | "x.no-such-type" => Ok((
                    [(header::CONTENT_TYPE, "application/octet-stream")],
                    "the file's bytes",
                )
                    .into_response()),
                "served.json" => Ok((
                    [(header::CONTENT_TYPE, "application/json")],
                    "{\"own\":\"type\"}",
                )
                    .into_response()),
                _ => Err(Problem::not_found()),
            }
        })
    }
}

/// What reached the fixture viewer server.
#[derive(Debug)]
struct Seen {
    method: Method,
    path: String,
    headers: HeaderMap,
}

/// A server that declares a viewer of `.usda` at `/beacon/viewer/`, and tries to set a cookie.
/// A server's answer on its content routes: its decision, or a file of no known type.
fn owned_answer(request: &Request, allowed: bool) -> Response {
    let links = request.uri().path().starts_with("/beacon/content-links/");
    if !allowed {
        let status = if links {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::FORBIDDEN
        };
        return status.into_response();
    }
    if request.uri().path().ends_with("/a1") || request.uri().path().ends_with(LINK_TOKEN) {
        return StatusCode::NO_CONTENT.into_response();
    }
    let status = if request.headers().contains_key(header::RANGE) {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    (
        status,
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::SET_COOKIE, "own=1; Path=/"),
        ],
        "the server's file",
    )
        .into_response()
}

async fn viewer_server(
    allows: Arc<AtomicBool>,
) -> (crate::registration::RegisteredServer, Arc<Mutex<Vec<Seen>>>) {
    let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
    let recorded = seen.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().fallback(move |request: Request| {
        let recorded = recorded.clone();
        let allows = allows.clone();
        async move {
            recorded.lock().unwrap().push(Seen {
                method: request.method().clone(),
                path: request.uri().to_string(),
                headers: request.headers().clone(),
            });
            let path = request.uri().path();
            if path.starts_with("/beacon/content/") || path.starts_with("/beacon/content-links/") {
                return owned_answer(&request, allows.load(Ordering::SeqCst));
            }
            (
                [
                    (header::SET_COOKIE, "viewer=own; Path=/"),
                    (header::CONTENT_SECURITY_POLICY, "script-src 'self'"),
                ],
                "the viewer's file",
            )
                .into_response()
        }
    });
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let endpoint = format!("http://{address}/beacon/mcp");
    let mut server = crate::test_catalog::registered_server("beacon", &endpoint);
    server.surfaces = Some(SurfacesDeclaration {
        public_routes: ["/beacon/viewer/", "/beacon/content-links/"]
            .map(|prefix| PublicRouteDeclaration {
                prefix: PublicRoutePrefix::new(prefix).unwrap(),
                max_body_bytes: BodyLimit::new(1).unwrap(),
                requests_per_minute: RequestRate::per_minute(6000).unwrap(),
            })
            .to_vec(),
        app_origin: false,
        viewer: Some(ViewerDeclaration {
            extensions: vec![FileExtension::new("usda").unwrap()],
            route: PublicRoutePrefix::new("/beacon/viewer/").unwrap(),
        }),
        content: Some(ContentDeclaration {
            route: PublicRoutePrefix::new("/beacon/content/").unwrap(),
            links: Some(PublicRoutePrefix::new("/beacon/content-links/").unwrap()),
        }),
    });
    (server, seen)
}

struct Harness {
    owner: AppsOwner,
    page: Router,
    listener: Router,
    allowed: Arc<AtomicBool>,
    reads: Arc<Mutex<Vec<String>>>,
    seen: Arc<Mutex<Vec<Seen>>>,
    /// Whether the fixture server admits a read of its item or link.
    server_allows: Arc<AtomicBool>,
    instances: Router,
    deployment: veoveo_types::DeploymentId,
    _files: tempfile::TempDir,
}

impl Harness {
    async fn start(db: &TestDb) -> Self {
        let owner = apps_owner(db, crate::apps::tests::catalog()).await;
        let catalog = HubCatalogHandle::new(Arc::new(owner.catalog.clone()));
        let server_allows = Arc::new(AtomicBool::new(true));
        let (server, seen) = viewer_server(server_allows.clone()).await;
        let deployment = server.deployment;
        catalog.replace_registrations(crate::registration::RegistrationSnapshot::new([server]));
        let files = tempfile::tempdir().unwrap();
        std::fs::create_dir(files.path().join("file-viewer")).unwrap();
        std::fs::write(files.path().join("file-viewer/engine.wasm"), b"\0asm").unwrap();
        let allowed = Arc::new(AtomicBool::new(true));
        let reads: Arc<Mutex<Vec<String>>> = Arc::default();
        let origins = AppOrigins::new(HOST, SANDBOX.parse().unwrap()).unwrap();
        let fixture: Arc<dyn ContentOwner> = Arc::new(Fixture {
            allowed: allowed.clone(),
            reads: reads.clone(),
        });
        let issuer = veoveo_hub_sdk::HubInternalTokenIssuer::new(
            veoveo_hub_sdk::TokenIssuer::parse(HOST).unwrap(),
            veoveo_hub_sdk::HubInternalSigningKey::new(
                base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    crate::apps::tests::SIGNING_KEY,
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let servers = ServerOrigins::new(
            catalog.clone(),
            owner.hub.clone(),
            origins.clone(),
            crate::apps::tests::profile_authentication(&owner.hub, &catalog),
            issuer,
            crate::HubUpstreamHttpClientPool::new(),
        );
        let public_routes = PublicRoutes::new(
            catalog.clone(),
            crate::HubUpstreamHttpClientPool::new(),
            "veoveo.example",
        )
        .unwrap();
        let domain = ContentDomain::new(
            catalog,
            owner.hub.clone(),
            &origins,
            CONTENT.parse().unwrap(),
            files.path().to_owned(),
            public_routes.clone(),
            servers,
            [("fixture".to_owned(), fixture)],
        )
        .unwrap();
        Self {
            page: domain.router(),
            listener: SandboxProxy::new(origins, None, Some(domain)).router(),
            owner,
            allowed,
            reads,
            seen,
            server_allows,
            instances: public_routes.router(),
            deployment,
            _files: files,
        }
    }

    /// Open a page as a signed-in browser's document navigation.
    async fn open(&self, path: &str) -> (StatusCode, HeaderMap, String) {
        let request = Request::get(path)
            .header("sec-fetch-mode", "navigate")
            .header("sec-fetch-dest", "document")
            .header(
                header::COOKIE,
                format!("__Host-computer={}", self.owner.cookie),
            )
            .body(Body::empty())
            .unwrap();
        send(&self.page, request).await
    }

    /// The token the page frames for `path`, and the frame's address.
    async fn minted(&self, path: &str) -> (String, String) {
        let (status, _, body) = self.open(path).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let src = attribute(&body, "src=\"https://");
        let token = src.split('.').next().unwrap().to_owned();
        assert!(token::well_formed(&token), "{src}");
        (token, format!("https://{src}"))
    }

    /// A request on `token`'s origin.
    async fn on_origin(
        &self,
        token: &str,
        path: &str,
        extra: &[(&str, &str)],
    ) -> (StatusCode, HeaderMap, String) {
        let mut request = Request::get(path).header(
            "x-forwarded-host",
            format!("{token}.veoveo-content.example"),
        );
        for (name, value) in extra {
            request = request.header(*name, *value);
        }
        send(&self.listener, request.body(Body::empty()).unwrap()).await
    }
}

async fn send(router: &Router, request: Request) -> (StatusCode, HeaderMap, String) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, headers, String::from_utf8_lossy(&body).into_owned())
}

/// What follows `start` in `body` up to its next `"`.
fn attribute<'a>(body: &'a str, start: &str) -> &'a str {
    let at = body
        .find(start)
        .unwrap_or_else(|| panic!("{start}: {body}"));
    let rest = &body[at + start.len()..];
    &rest[..rest.find('"').unwrap()]
}

fn policies(headers: &HeaderMap) -> Vec<String> {
    headers
        .get_all(header::CONTENT_SECURITY_POLICY)
        .iter()
        .map(|value| value.to_str().unwrap().to_owned())
        .collect()
}

async fn set_expiry(db: &TestDb, token: &str, expires: chrono::TimeDelta) {
    let digest = crate::computers::session_digest(token);
    sqlx::query(
        "UPDATE hub.content_token SET expires_at = $2, hard_expires_at = GREATEST(hard_expires_at, $2) WHERE digest = $1",
    )
    .bind(digest.as_str())
    .bind(chrono::Utc::now() + expires)
    .execute(db.a.pool())
    .await
    .unwrap();
}

async fn timed<F: std::future::Future<Output = ()>>(test: F) {
    tokio::time::timeout(std::time::Duration::from_secs(120), test)
        .await
        .unwrap();
}

#[test]
fn the_content_origin_is_spelled_as_an_origin_and_stands_apart() {
    let origin: ContentOrigin = "https://veoveo-bioma.space".parse().unwrap();
    assert_eq!(origin.to_string(), "https://veoveo-bioma.space");
    assert_eq!(origin.under("*"), "https://*.veoveo-bioma.space");
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-forwarded-host",
        "ABC.Veoveo-Bioma.space".parse().unwrap(),
    );
    assert_eq!(origin.label(&headers).as_deref(), Some("abc"));
    headers.insert("x-forwarded-host", "veoveo-bioma.space".parse().unwrap());
    assert_eq!(origin.label(&headers), None);
    headers.insert("x-forwarded-host", "a.other.space".parse().unwrap());
    assert_eq!(origin.label(&headers), None);
    headers.append("x-forwarded-host", "a.veoveo-bioma.space".parse().unwrap());
    assert_eq!(origin.label(&headers), None);
    for refused in [
        "https://veoveo-bioma.space/",
        "ftp://veoveo-bioma.space",
        "https://space",
        "https://Veoveo.space",
    ] {
        assert!(refused.parse::<ContentOrigin>().is_err(), "{refused}");
    }
    // The domain may not hold, or lie under, the issuer or the Apps domain.
    for (issuer, sandbox, content) in [
        (HOST, SANDBOX, "https://veoveo.example"),
        (HOST, SANDBOX, "https://files.veoveo.example"),
        (HOST, SANDBOX, "https://apps.veoveo.example"),
        (HOST, SANDBOX, "https://x.apps.veoveo.example"),
        (
            "https://a.b.example",
            "https://sandbox.a.example",
            "https://b.example",
        ),
    ] {
        let Ok(content) = content.parse::<ContentOrigin>() else {
            continue;
        };
        let origins = AppOrigins::new(issuer, sandbox.parse().unwrap()).unwrap();
        let apart = origins_apart(&origins, &content);
        assert!(!apart, "{content} beside {issuer} and {sandbox}");
    }
    let origins = AppOrigins::new(HOST, SANDBOX.parse().unwrap()).unwrap();
    assert!(origins_apart(&origins, &origin));
}

fn origins_apart(origins: &AppOrigins, content: &ContentOrigin) -> bool {
    super::check_apart(origins, content).is_ok()
}

#[test]
fn a_view_path_has_one_spelling() {
    let canonical = |path: &str| view_return_path(path);
    assert_eq!(
        canonical("/view/lore/r/main/a b.png").as_deref(),
        Some("/view/lore/r/main/a%20b.png")
    );
    assert_eq!(
        canonical("/view/lore/r/feature%2Fa/%41.png").as_deref(),
        Some("/view/lore/r/feature%2Fa/A.png")
    );
    assert_eq!(
        canonical("/view/lore/r/main/a;b,c").as_deref(),
        Some("/view/lore/r/main/a%3Bb%2Cc")
    );
    assert_eq!(
        canonical("/view/lore/r/main/").as_deref(),
        Some("/view/lore/r/main/")
    );
    let spelled = canonical("/view/lore/r/main/#%25[x]").unwrap();
    assert_eq!(
        view_return_path(&spelled).as_deref(),
        Some(spelled.as_str())
    );
    for refused in [
        "/view/",
        "/view/lore//main/a",
        "/view/lore/r/../a",
        "/view/lore/r/%2e%2E/a",
        "/view/lore/r/./a",
        "/apps/x/y",
        "/views/lore",
    ] {
        assert_eq!(canonical(refused), None, "{refused}");
    }
    assert_eq!(canonical(&format!("/view/{}", "a".repeat(2048))), None);
}

#[tokio::test]
async fn a_person_is_signed_in_and_the_page_frames_the_file_with_one_token() {
    timed(async {
        let db = TestDb::new().await;
        let harness = Harness::start(&db).await;

        // Signed out, a document navigation signs in and returns here; anything else is told.
        let request = |dest: &str| {
            Request::get("/view/fixture/box/site/a;b.html")
                .header("sec-fetch-mode", "navigate")
                .header("sec-fetch-dest", dest)
                .body(Body::empty())
                .unwrap()
        };
        let (status, headers, _) = send(&harness.page, request("document")).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(headers[header::LOCATION], "/computer");
        assert!(
            headers[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .contains("=/view/fixture/box/site/a%3Bb.html;"),
            "{headers:?}"
        );
        let (status, _, _) = send(&harness.page, request("iframe")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // Signed in: HTML loads as itself, in the one frame, under the page's policy.
        let (status, headers, body) = harness.open("/view/fixture/box/site/page.html").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            body.contains(
                "sandbox=\"allow-scripts allow-same-origin allow-forms allow-popups allow-downloads\""
            ),
            "{body}"
        );
        assert!(body.contains("allow=\"webgpu; autoplay\""), "{body}");
        let policy = &policies(&headers)[0];
        assert!(
            policy.contains("frame-src https://*.veoveo-content.example;"),
            "{policy}"
        );
        assert!(policy.contains("frame-ancestors 'none'"), "{policy}");
        assert_eq!(
            headers[header::CACHE_CONTROL],
            "private, no-store, no-transform"
        );
        let (token, src) = harness.minted("/view/fixture/box/site/page.html").await;
        assert_eq!(
            src,
            format!("https://{token}.veoveo-content.example/site/page.html")
        );

        // The token is only stored hashed, and reopening the item finds the same one, whatever
        // the file; another item of another spelling is not a token here.
        let (again, src) = harness.minted("/view/fixture/box/a%20b.png").await;
        assert_eq!(again, token);
        assert_eq!(
            src,
            format!("https://{token}.veoveo-content.example/_veoveo/view/a%20b.png")
        );
        let stored: (String, Vec<u8>) =
            sqlx::query_as("SELECT digest, nonce FROM hub.content_token")
                .fetch_one(db.a.pool())
                .await
                .unwrap();
        assert_eq!(stored.0, crate::computers::session_digest(&token).as_str());
        assert_eq!(stored.1.len(), token::NONCE_BYTES);

        // The owner decides what a person can mint.
        harness.allowed.store(false, Ordering::SeqCst);
        let (status, _, body) = harness.open("/view/fixture/box/site/page.html").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(body.contains("You cannot open this file"), "{body}");
        harness.allowed.store(true, Ordering::SeqCst);
        let (status, _, _) = harness.open("/view/fixture/other/page.html").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _, _) = harness.open("/view/nobody/box/page.html").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        // Renewal takes the issuer's exact origin, and answers the frame's origin.
        let renew = |origin: &str| {
            Request::post("/view/fixture/box/site/page.html")
                .header(header::ORIGIN, origin)
                .header(
                    header::COOKIE,
                    format!("__Host-computer={}", harness.owner.cookie),
                )
                .body(Body::empty())
                .unwrap()
        };
        let (status, _, _) = send(&harness.page, renew("https://evil.example")).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        set_expiry(&db, &token, chrono::TimeDelta::minutes(5)).await;
        let (status, _, body) = send(&harness.page, renew(HOST)).await;
        assert_eq!(status, StatusCode::OK);
        let answered: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            answered["origin"],
            format!("https://{token}.veoveo-content.example")
        );
        let expires: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT expires_at FROM hub.content_token")
                .fetch_one(db.a.pool())
                .await
                .unwrap();
        assert!(expires > chrono::Utc::now() + chrono::TimeDelta::minutes(50));
    })
    .await;
}

#[tokio::test]
async fn the_origin_answers_files_with_their_types_and_checks_access_each_time() {
    timed(async {
        let db = TestDb::new().await;
        let harness = Harness::start(&db).await;
        let (token, _) = harness.minted("/view/fixture/box/site/page.html").await;

        let (status, headers, body) = harness
            .on_origin(
                &token,
                "/site/page.html",
                // Whatever a file set on its own origin stays there.
                &[(header::COOKIE.as_str(), "own=1")],
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body, "the file's bytes");
        assert_eq!(headers[header::CONTENT_TYPE], "text/html");
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(headers[header::CACHE_CONTROL], "private, no-transform");
        assert_eq!(headers[header::REFERRER_POLICY], "no-referrer");
        assert_eq!(policies(&headers), [FRAMING]);

        // A name it cannot guess, an unknown extension and an owner's own type.
        let (_, headers, _) = harness.on_origin(&token, "/a%20b.png", &[]).await;
        assert_eq!(headers[header::CONTENT_TYPE], "image/png");
        let (_, headers, _) = harness.on_origin(&token, "/x.no-such-type", &[]).await;
        assert_eq!(headers[header::CONTENT_TYPE], "application/octet-stream");
        let (_, headers, _) = harness.on_origin(&token, "/served.json", &[]).await;
        assert_eq!(headers[header::CONTENT_TYPE], "application/json");
        let (status, headers, _) = harness.on_origin(&token, "/missing", &[]).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(policies(&headers), [FRAMING]);

        // Nothing but a token's own read-only item.
        for name in ["x1", &"0".repeat(40)] {
            let (status, _, _) = harness.on_origin(name, "/site/page.html", &[]).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{name}");
        }
        let post = Request::post("/site/page.html")
            .header(
                "x-forwarded-host",
                format!("{token}.veoveo-content.example"),
            )
            .body(Body::empty())
            .unwrap();
        let (status, _, _) = send(&harness.listener, post).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);

        // A revoked grant stops the open token on its next request, for every path.
        harness.allowed.store(false, Ordering::SeqCst);
        for path in [
            "/site/page.html",
            "/_veoveo/view/a.docx",
            "/_veoveo/viewer/file-viewer/engine.wasm",
            "/_veoveo/viewers/beacon/index.html",
        ] {
            let (status, _, _) = harness.on_origin(&token, path, &[]).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
        }
        harness.allowed.store(true, Ordering::SeqCst);
        let (status, _, _) = harness.on_origin(&token, "/site/page.html", &[]).await;
        assert_eq!(status, StatusCode::OK);

        // Ending the session stops it too; a page load goes to sign in.
        harness.owner.revoke().await;
        let (status, _, _) = harness.on_origin(&token, "/site/page.html", &[]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, headers, _) = harness
            .on_origin(
                &token,
                "/site/page.html",
                &[
                    ("sec-fetch-mode", "navigate"),
                    ("sec-fetch-dest", "document"),
                ],
            )
            .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            headers[header::LOCATION],
            "https://veoveo.example/view/fixture/box/site/page.html"
        );
    })
    .await;
}

#[tokio::test]
async fn the_viewers_are_chosen_by_type_and_served_on_the_origin() {
    timed(async {
        let db = TestDb::new().await;
        let harness = Harness::start(&db).await;
        let (token, src) = harness.minted("/view/fixture/box/dir/a%20b.docx").await;
        assert_eq!(
            src,
            format!("https://{token}.veoveo-content.example/_veoveo/view/dir/a%20b.docx")
        );

        // file-viewer's page: a charset, its script, the file, and a policy naming only here.
        let (status, headers, body) = harness
            .on_origin(&token, "/_veoveo/view/dir/a%20b.docx", &[])
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "text/html; charset=utf-8");
        // No HTML here may be rewritten on the way, as a CDN adding its beacon would.
        assert_eq!(
            headers
                .get_all(header::CACHE_CONTROL)
                .iter()
                .map(|value| value.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["private, no-store", "no-transform"]
        );
        assert!(body.contains("<meta charset=\"utf-8\">"), "{body}");
        assert!(
            body.contains("src=\"/_veoveo/viewer/flyfish-file-viewer-web-full.iife.js\""),
            "{body}"
        );
        assert!(
            body.contains("\"path\":\"/dir/a%20b.docx\",\"filename\":\"a b.docx\""),
            "{body}"
        );
        let policy = policies(&headers);
        assert!(
            policy[0].starts_with("default-src 'self' blob: data:;"),
            "{policy:?}"
        );
        assert!(policy[0].contains("'wasm-unsafe-eval'"), "{policy:?}");
        assert!(!policy[0].contains("https:"), "{policy:?}");
        assert_eq!(policy[1], FRAMING);

        // file-viewer's files, from its mounted image, with no way out of it.
        let (status, headers, body) = harness
            .on_origin(&token, "/_veoveo/viewer/file-viewer/engine.wasm", &[])
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "\0asm");
        assert_eq!(headers[header::CONTENT_TYPE], "application/wasm");
        assert_eq!(headers[header::CACHE_CONTROL], "private, max-age=86400");
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(policies(&headers), [FRAMING]);
        for path in [
            "/_veoveo/viewer/../token.rs",
            "/_veoveo/viewer/%2e%2e/%2e%2e/etc/passwd",
            "/_veoveo/viewer/nothing.js",
            "/_veoveo/viewer/file-viewer/",
            "/_veoveo/other",
        ] {
            let (status, _, _) = harness.on_origin(&token, path, &[]).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        }
        assert!(
            harness.reads.lock().unwrap().is_empty(),
            "nothing under /_veoveo/ is the item's"
        );

        // A folder shows as itself, as does HTML in any case.
        let (_, src) = harness.minted("/view/fixture/box/dir/").await;
        assert_eq!(src, format!("https://{token}.veoveo-content.example/dir/"));
        let (_, src) = harness.minted("/view/fixture/box/INDEX.HTM").await;
        assert_eq!(
            src,
            format!("https://{token}.veoveo-content.example/INDEX.HTM")
        );

        // A server's declared viewer takes its extension, in any case.
        let (_, src) = harness.minted("/view/fixture/box/s/a%20b.USDA").await;
        assert_eq!(
            src,
            format!("https://{token}.veoveo-content.example/_veoveo/view/s/a%20b.USDA")
        );
        let (status, headers, _) = harness
            .on_origin(&token, "/_veoveo/view/s/a%20b.USDA", &[])
            .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            headers[header::LOCATION],
            "/_veoveo/viewers/beacon/?file=%2Fs%2Fa%2520b%2EUSDA"
        );
        let (status, headers, body) = harness
            .on_origin(
                &token,
                "/_veoveo/viewers/beacon/app.js?v=1",
                &[
                    (header::COOKIE.as_str(), "own=1"),
                    (
                        header::REFERER.as_str(),
                        "https://x.veoveo-content.example/",
                    ),
                    (header::ORIGIN.as_str(), "https://x.veoveo-content.example"),
                ],
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "the viewer's file");
        assert!(headers.get(header::SET_COOKIE).is_none(), "{headers:?}");
        assert_eq!(policies(&headers), ["script-src 'self'", FRAMING]);
        {
            let seen = harness.seen.lock().unwrap();
            let [seen] = seen.as_slice() else {
                panic!("{seen:?}");
            };
            assert_eq!(seen.path, "/beacon/viewer/app.js?v=1");
            assert_eq!(seen.headers[header::HOST], "veoveo.example");
            for name in ["cookie", "referer", "origin", "x-forwarded-host"] {
                assert!(seen.headers.get(name).is_none(), "{name}: {seen:?}");
            }
        }
        // Only a declared viewer's route, and only reading.
        for path in [
            "/_veoveo/viewers/other/a.js",
            "/_veoveo/viewers/not a slug/a.js",
        ] {
            let (status, _, _) = harness
                .on_origin(&token, &path.replace(' ', "%20"), &[])
                .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        }
        let (status, _, _) = harness
            .on_origin(&token, "/_veoveo/viewers/beacon/../../mcp", &[])
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    })
    .await;
}

#[tokio::test]
async fn an_expired_token_recovers_through_the_page_and_never_comes_back() {
    timed(async {
        let db = TestDb::new().await;
        let harness = Harness::start(&db).await;
        let (token, _) = harness.minted("/view/fixture/box/site/page.html").await;
        set_expiry(&db, &token, chrono::TimeDelta::seconds(-1)).await;

        // A page load goes to the page; in the Hub's frame it says so; anything else is told.
        let navigate = [("sec-fetch-mode", "navigate"), ("sec-fetch-dest", "document")];
        let (status, headers, _) = harness.on_origin(&token, "/site/page.html", &navigate).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            headers[header::LOCATION],
            "https://veoveo.example/view/fixture/box/site/page.html"
        );
        let (status, headers, _) = harness
            .on_origin(&token, "/_veoveo/view/a%20b.docx", &navigate)
            .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            headers[header::LOCATION],
            "https://veoveo.example/view/fixture/box/a%20b.docx"
        );
        let (_, headers, _) = harness
            .on_origin(&token, "/_veoveo/viewers/beacon/?file=x", &navigate)
            .await;
        assert_eq!(
            headers[header::LOCATION],
            "https://veoveo.example/view/fixture/box/"
        );
        let framed = [("sec-fetch-mode", "navigate"), ("sec-fetch-dest", "iframe")];
        let (status, headers, body) = harness.on_origin(&token, "/site/page.html", &framed).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(body.contains("This preview expired"), "{body}");
        assert_eq!(policies(&headers), [FRAMING]);
        let (status, _, _) = harness.on_origin(&token, "/site/page.html", &[]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // Opening it again draws a new token, and the old stays dead.
        let (fresh, _) = harness.minted("/view/fixture/box/site/page.html").await;
        assert_ne!(fresh, token);
        let (status, _, _) = harness.on_origin(&fresh, "/site/page.html", &[]).await;
        assert_eq!(status, StatusCode::OK);
        let (status, _, _) = harness.on_origin(&token, "/site/page.html", &[]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        // A token cannot pass its hard limit, however often it is renewed.
        sqlx::query(
            "UPDATE hub.content_token SET hard_expires_at = now() + interval '1 minute', \
             expires_at = now() + interval '1 minute' WHERE digest = $1",
        )
        .bind(crate::computers::session_digest(&fresh).as_str())
        .execute(db.a.pool())
        .await
        .unwrap();
        let (same, _) = harness.minted("/view/fixture/box/site/page.html").await;
        assert_eq!(same, fresh);
        let (expires, limit): (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
            sqlx::query_as(
                "SELECT expires_at, hard_expires_at FROM hub.content_token WHERE digest = $1",
            )
            .bind(crate::computers::session_digest(&fresh).as_str())
            .fetch_one(db.a.pool())
            .await
            .unwrap();
        assert_eq!(expires, limit);

        // Past the retention, a row goes when another token is drawn.
        sqlx::query("UPDATE hub.content_token SET hard_expires_at = now() - interval '8 days', expires_at = now() - interval '8 days'")
            .execute(db.a.pool())
            .await
            .unwrap();
        let (last, _) = harness.minted("/view/fixture/box/site/page.html").await;
        assert_ne!(last, fresh);
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM hub.content_token")
            .fetch_one(db.a.pool())
            .await
            .unwrap();
        assert_eq!(rows, 1);
    })
    .await;
}

#[tokio::test]
async fn a_servers_item_is_read_as_the_person_who_opens_it_once() {
    timed(async {
        let db = TestDb::new().await;
        let harness = Harness::start(&db).await;
        let seen = |harness: &Harness| {
            std::mem::take(&mut *harness.seen.lock().unwrap())
                .into_iter()
                .map(|seen| (seen.method.to_string(), seen.path, seen.headers))
                .collect::<Vec<_>>()
        };

        // Opening the page is the visit, as the person, and frames the viewer on its own origin.
        let (token, src) = harness.minted("/view/beacon/a1/pic.png").await;
        assert_eq!(
            src,
            format!("https://{token}.veoveo-content.example/_veoveo/view/pic.png")
        );
        let visit = seen(&harness);
        assert_eq!(
            visit.len(),
            1,
            "{:?}",
            visit.iter().map(|v| &v.1).collect::<Vec<_>>()
        );
        assert_eq!(
            (visit[0].0.as_str(), visit[0].1.as_str()),
            ("POST", "/beacon/content/a1")
        );
        assert!(
            visit[0].2[header::AUTHORIZATION]
                .to_str()
                .unwrap()
                .starts_with("Bearer ")
        );

        // The page's renewal checks the person and is no visit.
        let renew = Request::post("/view/beacon/a1/pic.png")
            .header(header::ORIGIN, HOST)
            .header(
                header::COOKIE,
                format!("__Host-computer={}", harness.owner.cookie),
            )
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(&harness.page, renew).await.0, StatusCode::OK);
        let renewal = seen(&harness);
        assert_eq!(
            renewal
                .iter()
                .map(|v| (v.0.as_str(), v.1.as_str()))
                .collect::<Vec<_>>(),
            [("HEAD", "/beacon/content/a1")]
        );

        // A file goes to the server with its range and validators, and nothing that names the
        // origin; its type comes from its name, and what it sets stays with it.
        let (status, headers, body) = harness
            .on_origin(
                &token,
                "/pic.png",
                &[
                    ("range", "bytes=0-3"),
                    ("if-none-match", "\"a1\""),
                    ("referer", "https://elsewhere.example/"),
                    ("cookie", "own=1"),
                ],
            )
            .await;
        assert_eq!(status, StatusCode::PARTIAL_CONTENT);
        assert_eq!(body, "the server's file");
        assert_eq!(headers[header::CONTENT_TYPE], "image/png");
        assert_eq!(headers[header::CACHE_CONTROL], "private, no-transform");
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert!(headers.get(header::SET_COOKIE).is_none());
        assert_eq!(policies(&headers), [FRAMING]);
        let read = seen(&harness);
        assert_eq!(read.len(), 1);
        assert_eq!(
            (read[0].0.as_str(), read[0].1.as_str()),
            ("GET", "/beacon/content/a1/pic.png")
        );
        assert_eq!(read[0].2[header::RANGE], "bytes=0-3");
        assert_eq!(read[0].2[header::IF_NONE_MATCH], "\"a1\"");
        assert!(read[0].2.contains_key(header::AUTHORIZATION));
        for name in [header::REFERER, header::COOKIE, header::ORIGIN] {
            assert!(!read[0].2.contains_key(&name), "{name}");
        }

        // What the Hub serves itself asks the server first, and a refusal stops it.
        let viewer = harness
            .on_origin(&token, "/_veoveo/viewer/file-viewer/engine.wasm", &[])
            .await;
        assert_eq!(viewer.0, StatusCode::OK);
        assert_eq!(
            seen(&harness)
                .iter()
                .map(|v| (v.0.clone(), v.1.clone()))
                .collect::<Vec<_>>(),
            [("HEAD".to_owned(), "/beacon/content/a1".to_owned())]
        );

        // No path climbs out of the route, with the person's token, to the server's MCP.
        for path in ["/%2e%2e/%2e%2e/mcp", "/x%2Fy", "//x"] {
            let (status, ..) = harness.on_origin(&token, path, &[]).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        }
        assert!(seen(&harness).is_empty());

        // The server's refusal reaches the page, the file and the viewer's file at once.
        harness.server_allows.store(false, Ordering::SeqCst);
        assert_eq!(
            harness.on_origin(&token, "/pic.png", &[]).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            harness
                .on_origin(&token, "/_veoveo/viewer/file-viewer/engine.wasm", &[])
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let (status, _, body) = harness.open("/view/beacon/a2/pic.png").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(body.contains("You cannot open this file"), "{body}");
    })
    .await;
}

#[tokio::test]
async fn a_servers_link_has_its_own_origin_and_never_runs_on_the_issuers() {
    timed(async {
        let db = TestDb::new().await;
        let harness = Harness::start(&db).await;
        let name = format!("beacon-{LINK_TOKEN}");
        let seen = |harness: &Harness| {
            std::mem::take(&mut *harness.seen.lock().unwrap())
                .into_iter()
                .map(|seen| (seen.method.to_string(), seen.path, seen.headers))
                .collect::<Vec<_>>()
        };

        // The file comes from the link's route, without anything of the person or the browser.
        let (status, headers, body) = harness
            .on_origin(
                &name,
                "/pic.png",
                &[
                    ("range", "bytes=0-3"),
                    ("referer", "https://elsewhere.example/"),
                ],
            )
            .await;
        assert_eq!(status, StatusCode::PARTIAL_CONTENT);
        assert_eq!(body, "the server's file");
        assert_eq!(headers[header::CONTENT_TYPE], "image/png");
        assert_eq!(headers[header::CACHE_CONTROL], "private, no-transform");
        assert_eq!(policies(&headers), [FRAMING]);
        let read = seen(&harness);
        assert_eq!(
            (read[0].0.as_str(), read[0].1.clone()),
            ("GET", format!("/beacon/content-links/{LINK_TOKEN}/pic.png"))
        );
        assert_eq!(read[0].2[header::RANGE], "bytes=0-3");
        for name in [header::AUTHORIZATION, header::REFERER, header::COOKIE] {
            assert!(!read[0].2.contains_key(&name), "{name}");
        }

        // The viewer's page and files ask the server first.
        let (status, _, body) = harness.on_origin(&name, "/_veoveo/view/pic.png", &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("flyfish-file-viewer-web-full"), "{body}");
        assert_eq!(
            seen(&harness)
                .iter()
                .map(|v| (v.0.clone(), v.1.clone()))
                .collect::<Vec<_>>(),
            [(
                "HEAD".to_owned(),
                format!("/beacon/content-links/{LINK_TOKEN}")
            )]
        );

        // A write is refused, and a slug that owns no links names nothing.
        let write = Request::put("/pic.png")
            .header("x-forwarded-host", format!("{name}.veoveo-content.example"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            send(&harness.listener, write).await.0,
            StatusCode::METHOD_NOT_ALLOWED
        );
        let other = format!("other-{LINK_TOKEN}");
        assert_eq!(
            harness.on_origin(&other, "/pic.png", &[]).await.0,
            StatusCode::NOT_FOUND
        );
        assert!(seen(&harness).is_empty());

        // On the issuer's origin the file would run with a person's session, so it is not served
        // there, however it is spelled; the server's other public routes are.
        for path in [
            format!("/beacon/content-links/{LINK_TOKEN}/pic.html"),
            format!("/beacon/content-links/{LINK_TOKEN}"),
            format!("/beacon/%63ontent-links/{LINK_TOKEN}/pic.html"),
            format!("/beacon/viewer/../content-links/{LINK_TOKEN}/pic.html"),
        ] {
            let request = Request::get(format!("/instances/{}{path}", harness.deployment))
                .body(Body::empty())
                .unwrap();
            let (status, ..) = send(&harness.instances, request).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        }
        assert!(seen(&harness).is_empty());
        let request = Request::get(format!("/instances/{}/beacon/viewer/", harness.deployment))
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(&harness.instances, request).await.0, StatusCode::OK);
        assert_eq!(seen(&harness).len(), 1);

        // The server's refusal ends it at once: file, page and viewer's files alike.
        harness.server_allows.store(false, Ordering::SeqCst);
        for path in [
            "/pic.png",
            "/_veoveo/view/pic.png",
            "/_veoveo/viewer/file-viewer/engine.wasm",
        ] {
            let (status, ..) = harness.on_origin(&name, path, &[]).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        }
    })
    .await;
}

#[test]
fn a_factory_instance_a_person_reaches_opens_its_extensions() {
    let mut server =
        crate::test_catalog::registered_server("beacon", "http://127.0.0.1:9/beacon/mcp");
    server.branch = Some("main".parse().unwrap());
    server.surfaces = Some(SurfacesDeclaration {
        public_routes: vec![PublicRouteDeclaration {
            prefix: PublicRoutePrefix::new("/beacon/viewer/").unwrap(),
            max_body_bytes: BodyLimit::new(1).unwrap(),
            requests_per_minute: RequestRate::per_minute(6000).unwrap(),
        }],
        app_origin: false,
        viewer: Some(ViewerDeclaration {
            extensions: vec![FileExtension::new("usda").unwrap()],
            route: PublicRoutePrefix::new("/beacon/viewer/").unwrap(),
        }),
        content: None,
    });
    let servers = crate::registration::RegistrationSnapshot::new([server]);
    assert!(matches!(
        super::serve::viewer_for(&servers, "a/b.USDA"),
        super::serve::Viewer::Declared(_)
    ));
    assert!(matches!(
        super::serve::viewer_for(&servers, "a/b.docx"),
        super::serve::Viewer::File
    ));
}
