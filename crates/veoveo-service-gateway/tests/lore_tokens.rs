//! Runs the Gateway binary on an installation with a Lore server and checks over HTTP
//! that client-credentials tokens for Lore carry each client's repository grants in the
//! shape Lore's per-repository authorizer reads, verify with the keys the issuer's
//! OpenID discovery document names, and carry no scope.

#[path = "support/gateway.rs"]
mod gateway;

use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use gateway::Gateway;
use jsonwebtoken::{DecodingKey, Validation, jwk::JwkSet};
use reqwest::StatusCode;
use serde_json::{Value, json};
use veoveo_fixtures::store::TestDb;

const DEADLINE: Duration = Duration::from_secs(120);
const LORE: &str = "lores://vv-lore.internal:41337";
const REPOSITORY: &str = "01a1397efdb27152b7259d027ce59c45";

#[tokio::test]
async fn lore_tokens_carry_each_clients_repository_grants() {
    tokio::time::timeout(DEADLINE, lore_tokens())
        .await
        .expect("the Lore token checks exceeded two minutes");
}

async fn lore_tokens() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = jsonwebtoken::crypto::aws_lc::DEFAULT_PROVIDER.install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    // The member reaches one repository; the admin merges into every repository as
    // Lore's service account.
    let gateway = Gateway::start_with(
        directory.path(),
        &db,
        Duration::from_secs(60),
        &[],
        |config| {
            for client in config["oauthClients"].as_array_mut().unwrap() {
                client["allowedResources"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(LORE));
            }
            config["lore"] = json!({
                "resource": LORE,
                "authorizationServer": "local",
                "grants": [
                    { "client": "acceptance-member", "repositories": [REPOSITORY] },
                    {
                        "client": "acceptance-admin",
                        "allRepositories": true,
                        "actions": ["owner", "admin"],
                        "serviceAccount": true
                    }
                ]
            });
        },
    )
    .await;
    let http = reqwest::Client::new();
    let endpoint = gateway.token_endpoint();
    let lore_form = |role: &str| {
        let caller = gateway.caller(role);
        let client = Gateway::client(caller);
        let assertion = gateway
            .signer(caller)
            .assertion(&client, &endpoint)
            .unwrap();
        gateway
            .client_credentials(&client, assertion.expose_secret())
            .with("resource", LORE)
            .without("scope")
    };

    // Lore reads the issuer and its keys from OpenID Connect Discovery under the issuer.
    let issuer = format!("{}/oauth", gateway.base.as_str().trim_end_matches('/'));
    let discovery: Value = http
        .get(format!("{issuer}/.well-known/openid-configuration"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(discovery["issuer"], issuer.as_str());
    let keys: JwkSet = http
        .get(discovery["jwks_uri"].as_str().unwrap())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let (status, _, body) = gateway.request_token(&http, &lore_form("member")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let member = verified(body["access_token"].as_str().unwrap(), &keys, &issuer);
    assert_eq!(member["aud"], LORE);
    assert_eq!(member["sub"], "acceptance-member");
    assert_eq!(
        member["lore_resources"],
        json!([{ "resource_id": format!("urc-{REPOSITORY}"), "permission": [] }])
    );
    assert!(member.get("is_service_account").is_none());
    assert!(member.get("scope").is_none());

    let (status, _, body) = gateway.request_token(&http, &lore_form("admin")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let admin = verified(body["access_token"].as_str().unwrap(), &keys, &issuer);
    assert_eq!(
        admin["lore_resources"],
        json!([{ "resource_id": "urc-*", "permission": ["admin", "owner"] }])
    );
    assert_eq!(admin["is_service_account"], true);

    // A Lore token carries grants, not scopes.
    let (status, _, body) = gateway
        .request_token(&http, &lore_form("member").with("scope", "mcp"))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"], "invalid_scope");

    // The same client still receives its MCP token for the local profile.
    let caller = gateway.caller("member");
    let client = Gateway::client(caller);
    let assertion = gateway
        .signer(caller)
        .assertion(&client, &endpoint)
        .unwrap();
    let (status, _, body) = gateway
        .request_token(
            &http,
            &gateway.client_credentials(&client, assertion.expose_secret()),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mcp = claims(body["access_token"].as_str().unwrap());
    assert_eq!(mcp["aud"], gateway.installation.resource.as_str());
    assert!(mcp.get("lore_resources").is_none());
}

/// The token's claims after checking its RFC 9068 type, signature, issuer and Lore
/// audience against `keys`, as Lore does.
fn verified(token: &str, keys: &JwkSet, issuer: &str) -> Value {
    let header = jsonwebtoken::decode_header(token).unwrap();
    assert_eq!(header.typ.as_deref(), Some("at+jwt"));
    let key = keys
        .find(header.kid.as_deref().unwrap())
        .expect("the discovery document's keys hold the token's key");
    let mut validation = Validation::new(header.alg);
    validation.set_issuer(&[issuer]);
    validation.set_audience(&[LORE]);
    jsonwebtoken::decode::<Value>(token, &DecodingKey::from_jwk(key).unwrap(), &validation)
        .unwrap()
        .claims
}

fn claims(token: &str) -> Value {
    let payload = token.split('.').nth(1).unwrap();
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap()
}
