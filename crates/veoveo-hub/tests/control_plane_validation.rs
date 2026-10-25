//! Control-plane validation against the Hub binary. `POST
//! /admin/{profile}/control-plane/validate` answers a document as Hub would keep and serve
//! it, with its SHA-256: the `GET`'s for the active document however it is written, and
//! another for a changed one, which a publication over `If-Match` then makes active
//! exactly as answered. It publishes nothing. A document a publication refuses is
//! refused alike by both routes, with the reason, and a caller without `admin_write` is
//! refused validation as it is publication.

#[path = "support/database.rs"]
mod database;
#[path = "support/hub.rs"]
mod hub;

use std::time::Duration;

use reqwest::{StatusCode, header};
use serde_json::{Value, json};

/// How long the scenario may take, Hub start included.
const BOUND: Duration = Duration::from_secs(120);

#[tokio::test]
async fn validation_answers_what_a_publication_would_make_active_and_publishes_nothing() {
    tokio::time::timeout(BOUND, scenario())
        .await
        .expect("the validation scenario exceeded its bound");
}

/// The control plane as `GET` answers it, with its entity tag.
async fn read(http: &reqwest::Client, endpoint: &str, token: &str) -> (String, Value) {
    let response = http.get(endpoint).bearer_auth(token).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let tag = response.headers()[header::ETAG]
        .to_str()
        .unwrap()
        .to_owned();
    (tag, response.json().await.unwrap())
}

async fn answer(request: reqwest::RequestBuilder) -> (StatusCode, Value) {
    let response = request.send().await.unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

async fn scenario() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = database::TestDb::new().await;
    let hub = hub::Hub::start(directory.path(), &db, Duration::from_secs(60)).await;
    let http = reqwest::Client::new();
    let admin = hub.access_token(&http, hub.caller("admin")).await;
    let endpoint = format!("{}/admin/local/control-plane", hub.base);
    let validate = format!("{endpoint}/validate");

    let (tag, active) = read(&http, &endpoint, &admin).await;
    assert_eq!(
        tag,
        format!("\"{}\"", active["revisionId"].as_str().unwrap())
    );
    // The seed as written and the control plane as served are one document.
    let seed: Value =
        serde_json::from_slice(&std::fs::read(&hub.installation.config).unwrap()).unwrap();
    for document in [&seed, &active["controlPlane"]] {
        let (status, valid) = answer(http.post(&validate).bearer_auth(&admin).json(document)).await;
        assert_eq!(status, StatusCode::OK, "{valid}");
        assert_eq!(valid["status"], "valid");
        assert_eq!(valid["sha256"], active["sha256"]);
        assert_eq!(valid["controlPlane"], active["controlPlane"]);
        assert_eq!(valid["profiles"], active["profiles"]);
    }

    // A changed document answers its own form and hash, and nothing is published.
    let mut changed = seed.clone();
    changed["metadata"]["publisher"] = json!("validation");
    let (status, valid) = answer(http.post(&validate).bearer_auth(&admin).json(&changed)).await;
    assert_eq!(status, StatusCode::OK, "{valid}");
    assert_ne!(valid["sha256"], active["sha256"]);
    assert_eq!(valid["controlPlane"]["metadata"]["publisher"], "validation");
    let (unchanged, still) = read(&http, &endpoint, &admin).await;
    assert_eq!(unchanged, tag);
    assert_eq!(still["sha256"], active["sha256"]);

    // Publishing that document makes active exactly what validation answered.
    let (status, published) = answer(
        http.put(&endpoint)
            .bearer_auth(&admin)
            .header(header::IF_MATCH, &tag)
            .json(&changed),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{published}");
    assert_eq!(published["sha256"], valid["sha256"]);
    let (tag, active) = read(&http, &endpoint, &admin).await;
    assert_eq!(active["sha256"], valid["sha256"]);
    assert_eq!(active["controlPlane"], valid["controlPlane"]);

    // A document a publication refuses is refused alike, with the reason.
    let mut invalid = changed.clone();
    let rule = invalid["policies"][0]["rules"][0].clone();
    invalid["policies"][0]["rules"]
        .as_array_mut()
        .unwrap()
        .push(rule);
    let (status, refused) = answer(http.post(&validate).bearer_auth(&admin).json(&invalid)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    assert_eq!(
        refused["type"],
        "https://veoveo.ai/problems/control-plane-invalid"
    );
    assert!(
        refused["detail"]
            .as_str()
            .unwrap()
            .contains("duplicate policy rule"),
        "{refused}"
    );
    let (status, refused_publication) = answer(
        http.put(&endpoint)
            .bearer_auth(&admin)
            .header(header::IF_MATCH, &tag)
            .json(&invalid),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(refused_publication, refused);
    assert_eq!(read(&http, &endpoint, &admin).await.0, tag);

    // Validation is authorized as publication is.
    let member = hub.access_token(&http, hub.caller("member")).await;
    let (status, _) = answer(http.post(&validate).bearer_auth(&member).json(&changed)).await;
    let (publication, _) = answer(
        http.put(&endpoint)
            .bearer_auth(&member)
            .header(header::IF_MATCH, &tag)
            .json(&changed),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(publication, StatusCode::FORBIDDEN);
}
