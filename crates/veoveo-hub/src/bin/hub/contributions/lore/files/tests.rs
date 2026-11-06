//! The files answer against a stand-in `RevisionTree` and a local stand-in for Lore's HTTP
//! content route, whose bytes are synthetic. Connecting to Lore and issuing the token are
//! not exercised here; Lore's own tests refuse a real token on the real route.
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    body::Bytes,
    extract::{Path, Request},
    http::header::{ACCEPT, AUTHORIZATION, COOKIE, HeaderName},
};
use http_body_util::BodyExt;
use lore_proto::lore::model::v1::Address;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

use super::*;

const REPOSITORY: &str = "0123456789abcdef0123456789abcdef";
const TOKEN: &str = "the-persons-token";
const BODY: &[u8] = b"0123456789";

fn address_of(byte: u8) -> Address {
    Address {
        hash: vec![byte; 32].into(),
        context: vec![0x01; 8].into(),
    }
}

fn address_text(byte: u8) -> String {
    format!("{}-{}", hex::encode([byte; 32]), hex::encode([0x01; 8]))
}

/// The file stand-in Lore holds.
fn file_address() -> String {
    address_text(0xab)
}

/// A file stand-in Lore does not hold.
const LOST: u8 = 0xcd;
/// A file whose content route fails.
const BROKEN: u8 = 0xce;
/// A file stand-in Lore streams only as the test releases it.
const SLOW: u8 = 0xcf;

fn etag() -> String {
    format!("\"{}\"", file_address())
}

fn node(path: &str, kind: NodeType, size: u64, address: Option<Address>) -> TreeNode {
    TreeNode {
        path: path.to_owned(),
        node_type: kind as i32,
        address,
        size,
        mode: 0,
        tracking: false,
    }
}

/// A tree as Lore's listing of each folder names it.
struct Folders(BTreeMap<Option<&'static str>, Vec<TreeNode>>);

impl Tree for Folders {
    fn revision(&self) -> Option<String> {
        Some("tip-revision".to_owned())
    }

    fn folder<'a>(&'a self, folder: Option<&'a str>) -> BoxFuture<'a, Result<Vec<TreeNode>>> {
        Box::pin(async move {
            Ok(self
                .0
                .iter()
                .find(|(name, _)| **name == folder)
                .map(|(_, nodes)| nodes.clone())
                .unwrap_or_default())
        })
    }
}

fn tree() -> Folders {
    let file = |path: &str, byte: u8| node(path, NodeType::File, 10, Some(address_of(byte)));
    Folders(BTreeMap::from([
        (
            None,
            vec![
                node("scenes", NodeType::Directory, 20, None),
                file("readme.md", 0xab),
                node("linked", NodeType::Link, 0, Some(address_of(0x77))),
                file("lost.bin", LOST),
                file("broken.bin", BROKEN),
                file("slow.bin", SLOW),
                file("a#b?c%d [e].usd", 0xab),
            ],
        ),
        (
            Some("scenes"),
            vec![
                file("scenes/my kitchen.usda", 0xab),
                node("scenes/textures", NodeType::Directory, 10, None),
            ],
        ),
        (
            Some("scenes/textures"),
            vec![file("scenes/textures/é ✓.png", 0xab)],
        ),
        (Some("linked"), vec![file("linked/inside.usd", 0xab)]),
    ]))
}

/// What Lore's stand-in saw, and how a test releases its slow file.
#[derive(Clone, Default)]
struct StandIn {
    seen: Arc<Mutex<Vec<(String, HeaderMap)>>>,
    release: Arc<Mutex<Option<oneshot::Receiver<()>>>>,
}

/// A stand-in for `GET /v1/repository/{id}/content/{address}`: it answers Lore's statuses
/// and headers, and a content type that must not reach the caller.
async fn content_route(
    State(lore): State<StandIn>,
    Path((id, address)): Path<(String, String)>,
    request: Request,
) -> Response {
    lore.seen
        .lock()
        .unwrap()
        .push((request.uri().to_string(), request.headers().clone()));
    let bearer = request.headers().get(AUTHORIZATION);
    if id != REPOSITORY
        || bearer.and_then(|value| value.to_str().ok()) != Some("Bearer the-persons-token")
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if address == address_text(LOST) {
        return StatusCode::NOT_FOUND.into_response();
    }
    if address == address_text(BROKEN) {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if address == address_text(SLOW) {
        let release = lore.release.lock().unwrap().take().unwrap();
        let (sender, receiver) = mpsc::channel::<Result<Bytes, std::io::Error>>(1);
        tokio::spawn(async move {
            sender.send(Ok(Bytes::from_static(b"first"))).await.unwrap();
            release.await.unwrap();
            sender
                .send(Ok(Bytes::from_static(b"second")))
                .await
                .unwrap();
        });
        return Body::from_stream(ReceiverStream::new(receiver)).into_response();
    }
    let lied = [(CONTENT_TYPE, "text/html")];
    match request
        .headers()
        .get(RANGE)
        .map(|value| value.to_str().unwrap())
    {
        Some("bytes=2-4") => (
            StatusCode::PARTIAL_CONTENT,
            lied,
            [(CONTENT_RANGE, "bytes 2-4/10")],
            &BODY[2..5],
        )
            .into_response(),
        Some("bytes=100-") => (
            StatusCode::RANGE_NOT_SATISFIABLE,
            lied,
            [(CONTENT_RANGE, "bytes */10")],
        )
            .into_response(),
        _ => (lied, BODY).into_response(),
    }
}

struct Fixture {
    http: reqwest::Client,
    endpoint: url::Url,
    repository: RepositoryId,
    lore: StandIn,
}

async fn fixture() -> Fixture {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let lore = StandIn::default();
    let app = Router::new()
        .route("/v1/repository/{id}/content/{address}", get(content_route))
        .with_state(lore.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Fixture {
        http: client().unwrap(),
        endpoint: endpoint.parse().unwrap(),
        repository: RepositoryId::parse(REPOSITORY).unwrap(),
        lore,
    }
}

impl Fixture {
    async fn ask_as(
        &self,
        token: &str,
        tree: &dyn Tree,
        path: &str,
        headers: &[(HeaderName, &str)],
    ) -> Response {
        let path = FilePath::decode(path).expect(path);
        let content = Content {
            http: &self.http,
            endpoint: &self.endpoint,
            repository: &self.repository,
            token,
        };
        let mut request = HeaderMap::new();
        for (name, value) in headers {
            request.append(name.clone(), value.parse().unwrap());
        }
        marked(answer(tree, &content, "main", &path, &request).await)
    }

    async fn ask(&self, path: &str, headers: &[(HeaderName, &str)]) -> Response {
        self.ask_as(TOKEN, &tree(), path, headers).await
    }

    fn reads(&self) -> Vec<(String, HeaderMap)> {
        self.lore.seen.lock().unwrap().clone()
    }
}

async fn bytes(response: Response) -> Bytes {
    response.into_body().collect().await.unwrap().to_bytes()
}

async fn problem(response: Response) -> (StatusCode, String) {
    let status = response.status();
    assert_eq!(response.headers()[X_CONTENT_TYPE_OPTIONS], "nosniff");
    let body: Value = serde_json::from_slice(&bytes(response).await).unwrap();
    (status, body["type"].as_str().unwrap().to_owned())
}

fn is_problem(answer: &(StatusCode, String), status: StatusCode, code: &str) {
    assert_eq!(answer.0, status, "{}", answer.1);
    assert_eq!(answer.1, format!("https://veoveo.ai/problems/{code}"));
}

#[test]
fn a_path_decodes_one_segment_at_a_time() {
    for (encoded, names) in [
        ("", vec![]),
        (
            "my%20scene/file%20name.usd",
            vec!["my scene", "file name.usd"],
        ),
        ("caf%C3%A9/%E2%9C%93.usda", vec!["café", "✓.usda"]),
        ("café/✓.usda", vec!["café", "✓.usda"]),
        ("a%23b.usd", vec!["a#b.usd"]),
        ("a%3Fb.usd", vec!["a?b.usd"]),
        ("100%25.usd", vec!["100%.usd"]),
        ("100%", vec!["100%"]),
        ("%2541", vec!["%41"]),
        ("%5Bx%5D/y%5B1%5D.usd", vec!["[x]", "y[1].usd"]),
        ("[x]/y[1].usd", vec!["[x]", "y[1].usd"]),
        ("..filename/...", vec!["..filename", "..."]),
        ("folder/", vec!["folder"]),
    ] {
        assert_eq!(
            FilePath::decode(encoded).expect(encoded).0,
            names,
            "{encoded}"
        );
    }
    for encoded in [
        "..",
        "../x",
        "a/../b",
        "a/..",
        "%2e%2e",
        "%2E%2e/x",
        "a/%2e%2E/b",
        ".",
        "a/./b",
        "%2e",
        "a//b",
        "a/b//",
        "/a",
        "a%2Fb",
        "..%2Fb",
        "a%5Cb",
        "..%5Cb",
        "%00",
        "%FF",
    ] {
        assert_eq!(FilePath::decode(encoded), None, "{encoded}");
    }
}

#[test]
fn a_paths_folder_is_the_one_lore_lists_it_in() {
    let path = |encoded| FilePath::decode(encoded).unwrap();
    assert_eq!(path("a").parent(), None);
    assert_eq!(path("a/b").parent().as_deref(), Some("a"));
    assert_eq!(path("a/b%20c/d").parent().as_deref(), Some("a/b c"));
    assert_eq!(path("a/b%20c/d").tree_path(), "a/b c/d");
    assert_eq!(path("").parent(), None);
    assert!(path("").is_root());
}

#[test]
fn a_url_names_its_repository_branch_and_path() {
    let prefix = format!("/lore/local/repositories/{REPOSITORY}/files");
    for (tail, branch, names) in [
        ("/main", "main", vec![]),
        ("/main/", "main", vec![]),
        ("/main/a%20b/c%2523.usd", "main", vec!["a b", "c%23.usd"]),
        ("/feature%2Fx/a", "feature/x", vec!["a"]),
    ] {
        let url = format!("{prefix}{tail}");
        let (id, named_branch, path) = named(&url).expect(tail);
        assert_eq!(id, REPOSITORY);
        assert_eq!(named_branch, branch);
        assert_eq!(path.0, names, "{tail}");
    }
    for tail in ["/main/%2e%2e", "/main/a%2Fb", "/%FF/a"] {
        assert!(named(&format!("{prefix}{tail}")).is_none(), "{tail}");
    }
}

#[tokio::test]
async fn a_file_answers_its_bytes_with_its_address_as_the_etag() {
    let fixture = fixture().await;
    let response = fixture.ask("scenes/my%20kitchen.usda", &[]).await;
    assert_eq!(response.headers()["lore-revision"], "tip-revision");
    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers().clone();
    assert_eq!(headers[ETAG], etag().as_str());
    assert_eq!(headers[X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(headers[CACHE_CONTROL], "private, no-transform");
    assert_eq!(headers[ACCEPT_RANGES], "bytes");
    assert_eq!(headers[CONTENT_TYPE], "application/octet-stream");
    assert_eq!(headers[CONTENT_LENGTH], "10");
    assert_eq!(bytes(response).await.as_ref(), BODY);

    let reads = fixture.reads();
    assert_eq!(reads.len(), 1);
    assert_eq!(
        reads[0].0,
        format!("/v1/repository/{REPOSITORY}/content/{}", file_address()),
        "no query reaches Lore, which would let a caller choose its content type"
    );
    assert_eq!(
        reads[0].1[AUTHORIZATION],
        format!("Bearer {TOKEN}").as_str()
    );
}

#[tokio::test]
async fn names_with_url_characters_match_as_the_tree_holds_them() {
    let fixture = fixture().await;
    for encoded in [
        "a%23b%3Fc%25d%20%5Be%5D.usd",
        "scenes/textures/%C3%A9%20%E2%9C%93.png",
    ] {
        let response = fixture.ask(encoded, &[]).await;
        assert_eq!(response.status(), StatusCode::OK, "{encoded}");
        assert_eq!(bytes(response).await.as_ref(), BODY);
    }
}

#[tokio::test]
async fn only_the_range_of_a_request_reaches_lore() {
    let fixture = fixture().await;
    let response = fixture
        .ask(
            "readme.md",
            &[
                (RANGE, "bytes=2-4"),
                (AUTHORIZATION, "Bearer the-callers-hub-token"),
                (COOKIE, "session=the-callers"),
                (CONTENT_TYPE, "text/html"),
                (ACCEPT, "text/html"),
            ],
        )
        .await;
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()[CONTENT_RANGE], "bytes 2-4/10");
    assert_eq!(response.headers()[CONTENT_LENGTH], "3");
    assert_eq!(response.headers()[CONTENT_TYPE], "application/octet-stream");
    assert_eq!(bytes(response).await.as_ref(), b"234");
    let (_, seen) = &fixture.reads()[0];
    assert_eq!(seen[AUTHORIZATION], format!("Bearer {TOKEN}").as_str());
    assert_eq!(seen[RANGE], "bytes=2-4");
    assert!(seen.get(COOKIE).is_none());
    assert!(seen.get(CONTENT_TYPE).is_none());
    assert_ne!(
        seen.get(ACCEPT).map(|value| value.as_bytes()),
        Some(&b"text/html"[..])
    );
}

#[tokio::test]
async fn a_range_past_the_file_answers_lores_416() {
    let fixture = fixture().await;
    let response = fixture.ask("readme.md", &[(RANGE, "bytes=100-")]).await;
    assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(response.headers()[CONTENT_RANGE], "bytes */10");
    assert_eq!(bytes(response).await.len(), 0);
}

#[tokio::test]
async fn if_range_keeps_the_range_only_for_the_files_own_etag() {
    let fixture = fixture().await;
    let tag = etag();
    let response = fixture
        .ask("readme.md", &[(RANGE, "bytes=2-4"), (IF_RANGE, &tag)])
        .await;
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    for stale in ["\"another\"", "Wed, 21 Oct 2026 07:28:00 GMT"] {
        let response = fixture
            .ask("readme.md", &[(RANGE, "bytes=2-4"), (IF_RANGE, stale)])
            .await;
        assert_eq!(response.status(), StatusCode::OK, "{stale}");
        assert_eq!(bytes(response).await.as_ref(), BODY);
    }
    let reads = fixture.reads();
    assert!(reads[0].1.contains_key(RANGE));
    assert!(!reads[1].1.contains_key(RANGE));
    assert!(reads.iter().all(|(_, seen)| !seen.contains_key(IF_RANGE)));
}

#[tokio::test]
async fn an_unchanged_file_answers_304_and_lore_sends_nothing() {
    let fixture = fixture().await;
    let tag = etag();
    for held in [
        tag.clone(),
        format!("W/{tag}"),
        format!("\"another\", {tag}"),
        "*".to_owned(),
    ] {
        let response = fixture.ask("readme.md", &[(IF_NONE_MATCH, &held)]).await;
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED, "{held}");
        assert_eq!(response.headers()[ETAG], tag.as_str());
        assert_eq!(response.headers()[X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(response.headers()[CACHE_CONTROL], "private, no-transform");
        assert_eq!(bytes(response).await.len(), 0);
    }
    assert!(fixture.reads().is_empty());

    let response = fixture
        .ask("readme.md", &[(IF_NONE_MATCH, "\"another\"")])
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(fixture.reads().len(), 1);
}

#[tokio::test]
async fn bytes_reach_the_caller_as_lore_sends_them() {
    let fixture = fixture().await;
    let (release, released) = oneshot::channel();
    *fixture.lore.release.lock().unwrap() = Some(released);
    let response = fixture.ask("slow.bin", &[]).await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    let first = tokio::time::timeout(Duration::from_secs(10), body.frame())
        .await
        .expect("the first bytes arrive while Lore holds the rest")
        .unwrap()
        .unwrap();
    assert_eq!(first.into_data().unwrap().as_ref(), b"first");
    release.send(()).unwrap();
    let rest = body.collect().await.unwrap().to_bytes();
    assert_eq!(rest.as_ref(), b"second");
}

#[tokio::test]
async fn a_folder_answers_its_listing() {
    let fixture = fixture().await;
    let listing = |path: &'static str| {
        let fixture = &fixture;
        async move {
            let response = fixture.ask(path, &[]).await;
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(response.headers()[X_CONTENT_TYPE_OPTIONS], "nosniff");
            assert_eq!(response.headers()[CACHE_CONTROL], "private, no-transform");
            assert_eq!(response.headers()[CONTENT_TYPE], "application/json");
            serde_json::from_slice::<Value>(&bytes(response).await).unwrap()
        }
    };
    let expected = json!({
        "repository": REPOSITORY,
        "branch": "main",
        "path": "scenes",
        "entries": [
            {"name": "my kitchen.usda", "type": "file", "size": 10, "address": file_address()},
            {"name": "textures", "type": "directory", "size": 10},
        ],
    });
    assert_eq!(listing("scenes").await, expected);
    assert_eq!(listing("scenes/").await, expected);

    let root = listing("").await;
    assert_eq!(root["path"], "");
    assert_eq!(root["entries"].as_array().unwrap().len(), 7);
    assert_eq!(root["entries"][0]["name"], "a#b?c%d [e].usd");

    // A link lists as the folder it links to.
    assert_eq!(
        listing("linked").await["entries"],
        json!([{"name": "inside.usd", "type": "file", "size": 10, "address": file_address()}])
    );
    assert!(fixture.reads().is_empty());
}

#[tokio::test]
async fn what_the_tree_does_not_hold_is_not_found() {
    let fixture = fixture().await;
    for path in [
        "nothing.usd",
        "scenes/nothing",
        "nothing/at/all",
        "readme.md/below-a-file",
        "SCENES",
    ] {
        let answer = problem(fixture.ask(path, &[]).await).await;
        is_problem(&answer, StatusCode::NOT_FOUND, "lore-file-not-found");
    }
    assert!(fixture.reads().is_empty());
}

struct Failing(fn() -> anyhow::Error);

impl Tree for Failing {
    fn revision(&self) -> Option<String> {
        None
    }

    fn folder<'a>(&'a self, _: Option<&'a str>) -> BoxFuture<'a, Result<Vec<TreeNode>>> {
        Box::pin(async move { Err(self.0()) })
    }
}

fn answered(code: tonic::Code) -> anyhow::Error {
    anyhow::Error::new(Answered {
        code,
        message: "synthetic".to_owned(),
    })
}

#[tokio::test]
async fn lores_refusals_and_failures_answer_as_the_callers_errors() {
    let fixture = fixture().await;
    let on_tree = async |failure: fn() -> anyhow::Error| {
        problem(fixture.ask_as(TOKEN, &Failing(failure), "a", &[]).await).await
    };
    is_problem(
        &on_tree(|| anyhow::Error::new(Refused("synthetic".to_owned()))).await,
        StatusCode::FORBIDDEN,
        "repository-access-forbidden",
    );
    is_problem(
        &on_tree(|| answered(tonic::Code::NotFound)).await,
        StatusCode::NOT_FOUND,
        "lore-file-not-found",
    );
    is_problem(
        &on_tree(|| answered(tonic::Code::InvalidArgument)).await,
        StatusCode::NOT_FOUND,
        "lore-file-not-found",
    );
    is_problem(
        &on_tree(|| answered(tonic::Code::Internal)).await,
        StatusCode::SERVICE_UNAVAILABLE,
        "lore-files-unavailable",
    );
    is_problem(
        &on_tree(|| anyhow::anyhow!("no connection")).await,
        StatusCode::SERVICE_UNAVAILABLE,
        "lore-files-unavailable",
    );

    // Lore's content route decides as well: a token it refuses reads nothing.
    let refused = problem(
        fixture
            .ask_as("a-token-for-another-repository", &tree(), "readme.md", &[])
            .await,
    )
    .await;
    is_problem(
        &refused,
        StatusCode::FORBIDDEN,
        "repository-access-forbidden",
    );
    is_problem(
        &problem(fixture.ask("lost.bin", &[]).await).await,
        StatusCode::NOT_FOUND,
        "lore-file-not-found",
    );
    is_problem(
        &problem(fixture.ask("broken.bin", &[]).await).await,
        StatusCode::SERVICE_UNAVAILABLE,
        "lore-files-unavailable",
    );
}

#[tokio::test]
async fn a_lore_that_cannot_be_reached_answers_unavailable() {
    let mut fixture = fixture().await;
    // Nothing listens on a port this process just gave up.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    fixture.endpoint = format!("http://{}", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    drop(listener);
    is_problem(
        &problem(fixture.ask("readme.md", &[]).await).await,
        StatusCode::SERVICE_UNAVAILABLE,
        "lore-files-unavailable",
    );
}

#[test]
fn a_whole_tree_names_paths_and_does_not_enter_a_link() {
    let node = |path: &str, kind: NodeType| TreeNode {
        path: path.to_owned(),
        node_type: kind as i32,
        size: 3,
        ..TreeNode::default()
    };
    let entries = tree_entries(vec![
        node("", NodeType::Directory),
        node("a", NodeType::Directory),
        node("a/b.png", NodeType::File),
        node("a/linked", NodeType::Link),
        node("a/linked/secret.png", NodeType::File),
        node("a/linked/deep/secret.png", NodeType::File),
        node("a/linked-not.png", NodeType::File),
    ]);
    let paths: Vec<(&str, &str)> = entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry.kind))
        .collect();
    assert_eq!(
        paths,
        [
            ("a", "directory"),
            ("a/b.png", "file"),
            ("a/linked", "link"),
            ("a/linked-not.png", "file"),
        ]
    );
}
