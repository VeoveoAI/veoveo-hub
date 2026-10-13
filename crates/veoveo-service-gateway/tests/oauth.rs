//! Runs the Gateway binary against a disposable PostgreSQL database and checks over
//! HTTP that its token endpoint answers with the error codes of RFC 6749, RFC 7523 and
//! RFC 8707, and that its MCP endpoint challenges as RFC 6750 and RFC 9728 define.

#[path = "support/gateway.rs"]
mod gateway;

use std::time::Duration;

use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use chrono::{TimeDelta, Utc};
use gateway::{Gateway, TokenForm};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use reqwest::{
    StatusCode,
    header::{ACCEPT, AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, WWW_AUTHENTICATE},
};
use serde_json::{Value, json};
use veoveo_mcp_contract::{GatewayJwtRevocation, GatewayProfileId, JwtId, TokenIssuer};
use veoveo_mcp_ext_registration::statement::ServerKeyPair;
use veoveo_oauth_client::{ClientAssertionKeyId, ClientAssertionSigner};
use veoveo_service_gateway::GatewayState;
use veoveo_test_fixtures::store::TestDb;
use veoveo_types::OAuthClientId;

const DEADLINE: Duration = Duration::from_secs(120);

/// One running Gateway. Fields drop in order, so the process ends before its database.
struct Installation {
    gateway: Gateway,
    db: TestDb,
    _directory: tempfile::TempDir,
}

impl Installation {
    async fn start() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let _ = jsonwebtoken::crypto::aws_lc::DEFAULT_PROVIDER.install_default();
        let directory = tempfile::tempdir().unwrap();
        let db = TestDb::new().await;
        let gateway = Gateway::start(directory.path(), &db, Duration::from_secs(60)).await;
        Self {
            gateway,
            db,
            _directory: directory,
        }
    }
}

#[tokio::test]
async fn token_endpoint_errors_carry_the_codes_their_rfcs_define() {
    tokio::time::timeout(DEADLINE, token_endpoint_errors())
        .await
        .expect("the token endpoint checks exceeded two minutes");
}

async fn token_endpoint_errors() {
    let local = Installation::start().await;
    let gateway = &local.gateway;
    let http = reqwest::Client::new();
    let member = gateway.caller("member");
    let client = Gateway::client(member);
    let signer = gateway.signer(member);
    let endpoint = gateway.token_endpoint();
    let fresh = || {
        let assertion = signer.assertion(&client, &endpoint).unwrap();
        gateway.client_credentials(&client, assertion.expose_secret())
    };

    let form = fresh();
    let (status, headers, body) = gateway.request_token(&http, &form).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["scope"], gateway.installation.scope);
    assert_eq!(headers[CACHE_CONTROL], "no-store");
    // RFC 7523 §3 item 7: an assertion's `jti` authenticates once.
    expect_oauth_error(gateway, &http, &form, "invalid_client").await;

    // RFC 7523 §3.2: an assertion that fails its signature, lifetime or audience
    // check fails client authentication.
    let impostor = ClientAssertionSigner::from_rsa_pem(
        ClientAssertionKeyId::parse(member.key_id.clone()).unwrap(),
        &std::fs::read(&gateway.caller("admin").key).unwrap(),
    )
    .unwrap();
    let now = u64::try_from(Utc::now().timestamp()).unwrap();
    for (case, assertion) in [
        ("bad signature", impostor.assertion(&client, &endpoint)),
        (
            "expired",
            signer.diagnostic_assertion(&client, endpoint.as_str(), now - 600, now - 300, &jti()),
        ),
        (
            "wrong audience",
            signer.diagnostic_assertion(
                &client,
                "https://elsewhere.example/oauth/token",
                now,
                now + 60,
                &jti(),
            ),
        ),
    ] {
        let form = gateway.client_credentials(&client, assertion.unwrap().expose_secret());
        let (status, _, body) = gateway.request_token(&http, &form).await;
        assert_eq!(
            (status, &body["error"]),
            (StatusCode::BAD_REQUEST, &json!("invalid_client")),
            "{case}: {body}"
        );
    }

    // RFC 6749 §5.2: a client the installation does not register.
    let stranger = OAuthClientId::parse("unregistered-caller").unwrap();
    let assertion = signer.assertion(&stranger, &endpoint).unwrap();
    let form = gateway.client_credentials(&stranger, assertion.expose_secret());
    expect_oauth_error(gateway, &http, &form, "invalid_client").await;

    // RFC 8707 §2: a resource the client may not request.
    let elsewhere = format!("{}/mcp/elsewhere", gateway.base);
    expect_oauth_error(
        gateway,
        &http,
        &fresh().with("resource", elsewhere),
        "invalid_target",
    )
    .await;
    // RFC 6749 §5.2: a scope the client may not request, a grant type the Gateway
    // does not support and a request without its grant type.
    expect_oauth_error(
        gateway,
        &http,
        &fresh().with("scope", "admin"),
        "invalid_scope",
    )
    .await;
    expect_oauth_error(
        gateway,
        &http,
        &fresh().with("grant_type", "password"),
        "unsupported_grant_type",
    )
    .await;
    expect_oauth_error(
        gateway,
        &http,
        &fresh().without("grant_type"),
        "invalid_request",
    )
    .await;

    // RFC 7521 §4.2: the assertion identifies the client, so `client_id` is optional.
    let (status, _, body) = gateway
        .request_token(&http, &fresh().without("client_id"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// RFC 6749 §5.2: HTTP 400, a JSON body with the error code, and no caching. The
/// token endpoint supports no HTTP authentication scheme, so `invalid_client` is a 400
/// too.
async fn expect_oauth_error(
    gateway: &Gateway,
    http: &reqwest::Client,
    form: &TokenForm,
    code: &str,
) {
    let (status, headers, body) = gateway.request_token(http, form).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{code}: {body}");
    assert_eq!(body["error"], code, "{body}");
    assert_eq!(headers[CACHE_CONTROL], "no-store");
    assert_eq!(headers[CONTENT_TYPE], "application/json");
}

fn jti() -> String {
    uuid::Uuid::now_v7().to_string()
}

#[tokio::test]
async fn mcp_denials_carry_rfc_6750_challenges() {
    tokio::time::timeout(DEADLINE, bearer_challenges())
        .await
        .expect("the bearer challenge checks exceeded two minutes");
}

async fn bearer_challenges() {
    let local = Installation::start().await;
    let gateway = &local.gateway;
    let http = reqwest::Client::new();
    let member = gateway.caller("member");
    let token = gateway.access_token(&http, member).await;

    let discovered = mcp(&http, gateway, Some(&bearer(&token)), DISCOVER).await;
    assert_eq!(discovered.status(), StatusCode::OK);

    // RFC 6750 §3.1: a request without bearer credentials receives no error code.
    expect_challenge(gateway, mcp(&http, gateway, None, DISCOVER).await, None).await;
    let basic = format!("Basic {}", STANDARD.encode("member:password"));
    expect_challenge(
        gateway,
        mcp(&http, gateway, Some(&basic), DISCOVER).await,
        None,
    )
    .await;
    expect_challenge(
        gateway,
        mcp(&http, gateway, Some("Bearer two tokens"), DISCOVER).await,
        Some(("invalid_request", StatusCode::BAD_REQUEST)),
    )
    .await;

    let forger = Forger::new(gateway, &token);
    let now = Utc::now().timestamp();
    let other_key = ServerKeyPair::generate().unwrap();
    for (case, authorization) in [
        ("malformed", "Bearer not-a-jwt".to_owned()),
        (
            "expired",
            forger.sign(|claims| {
                claims["iat"] = json!(now - 600);
                claims["nbf"] = json!(now - 600);
                claims["exp"] = json!(now - 120);
            }),
        ),
        (
            "wrong audience",
            forger.sign(|claims| claims["aud"] = json!(format!("{}/mcp/other", gateway.base))),
        ),
        (
            "wrong issuer",
            forger.sign(|claims| claims["iss"] = json!(format!("{}/other", gateway.base))),
        ),
        ("wrong type", forger.with_type("JWT")),
        (
            "badly signed",
            forger.with_key(&EncodingKey::from_ed_der(other_key.pkcs8_der())),
        ),
    ] {
        let response = mcp(&http, gateway, Some(&authorization), DISCOVER).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{case}");
        expect_challenge(
            gateway,
            response,
            Some(("invalid_token", StatusCode::UNAUTHORIZED)),
        )
        .await;
    }

    // RFC 6750 §3.1: a valid token without the profile's scope is forbidden.
    let unscoped = forger.sign(|claims| claims["scope"] = json!("audit:read"));
    expect_challenge(
        gateway,
        mcp(&http, gateway, Some(&unscoped), DISCOVER).await,
        Some(("insufficient_scope", StatusCode::FORBIDDEN)),
    )
    .await;

    // A revocation every replica reads denies the token it names.
    let revoked = gateway.access_token(&http, member).await;
    let claims = claims(&revoked);
    let now = Utc::now();
    GatewayState::new(local.db.a.clone())
        .record_jwt_revocation(&GatewayJwtRevocation {
            profile: GatewayProfileId::parse(veoveo_service_gateway::local::LOCAL_PROFILE).unwrap(),
            issuer: TokenIssuer::parse(claims["iss"].as_str().unwrap()).unwrap(),
            jwt_id: JwtId::parse(claims["jti"].as_str().unwrap()).unwrap(),
            revoked_at: now,
            expires_at: now + TimeDelta::hours(1),
            reason: Some("bearer challenge qualification".to_owned()),
        })
        .await
        .unwrap();
    expect_challenge(
        gateway,
        mcp(&http, gateway, Some(&bearer(&revoked)), DISCOVER).await,
        Some(("invalid_token", StatusCode::UNAUTHORIZED)),
    )
    .await;
    let still_valid = mcp(&http, gateway, Some(&bearer(&token)), DISCOVER).await;
    assert_eq!(still_valid.status(), StatusCode::OK);
}

/// RFC 6750 §3 and RFC 9728 §5.1: a `Bearer` challenge naming the protected
/// resource's metadata and the scope it requires, with an RFC 9457 body.
async fn expect_challenge(
    gateway: &Gateway,
    response: reqwest::Response,
    error: Option<(&str, StatusCode)>,
) {
    let status = error.map_or(StatusCode::UNAUTHORIZED, |(_, status)| status);
    assert_eq!(response.status(), status);
    let challenge = response.headers()[WWW_AUTHENTICATE]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(challenge.starts_with("Bearer "), "{challenge}");
    let metadata = format!(
        "{}/.well-known/oauth-protected-resource/mcp/{}",
        gateway.base,
        veoveo_service_gateway::local::LOCAL_PROFILE
    );
    assert!(
        challenge.contains(&format!(r#"resource_metadata="{metadata}""#)),
        "{challenge}"
    );
    assert!(
        challenge.contains(&format!(r#"scope="{}""#, gateway.installation.scope)),
        "{challenge}"
    );
    match error {
        Some((code, _)) => assert!(
            challenge.contains(&format!(r#"error="{code}""#)),
            "{challenge}"
        ),
        None => assert!(!challenge.contains("error="), "{challenge}"),
    }
    assert_eq!(
        response.headers()[CONTENT_TYPE],
        veoveo_service_gateway::problem::PROBLEM_JSON
    );
    let problem: Value = response.json().await.unwrap();
    assert_eq!(problem["status"], status.as_u16());
}

const DISCOVER: Mcp = Mcp {
    method: "server/discover",
    uri: None,
    version: "2026-07-28",
};

/// One MCP request, with the HTTP headers and `_meta` members of its version.
#[derive(Clone, Copy)]
struct Mcp {
    method: &'static str,
    uri: Option<&'static str>,
    version: &'static str,
}

async fn mcp(
    http: &reqwest::Client,
    gateway: &Gateway,
    authorization: Option<&str>,
    request: Mcp,
) -> reqwest::Response {
    let mut params = json!({
        "_meta": {
            "io.modelcontextprotocol/protocolVersion": request.version,
            "io.modelcontextprotocol/clientInfo": { "name": "oauth-qualification", "version": "0.1.0" },
            "io.modelcontextprotocol/clientCapabilities": {}
        }
    });
    let mut builder = http
        .post(&gateway.installation.resource)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json, text/event-stream")
        .header("MCP-Protocol-Version", request.version)
        .header("Mcp-Method", request.method);
    if let Some(uri) = request.uri {
        params["uri"] = json!(uri);
        builder = builder.header("Mcp-Name", uri);
    }
    if let Some(authorization) = authorization {
        builder = builder.header(AUTHORIZATION, authorization);
    }
    builder
        .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": request.method, "params": params }))
        .send()
        .await
        .unwrap()
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// The claims of a JWT, decoded without verification.
fn claims(token: &str) -> Value {
    let payload = token.split('.').nth(1).unwrap();
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap()
}

/// Re-signs a token the Gateway issued with one change, using the hosted authorization
/// server's own key unless a case replaces it.
struct Forger {
    key_id: String,
    claims: Value,
    key: EncodingKey,
}

impl Forger {
    fn new(gateway: &Gateway, token: &str) -> Self {
        let header = jsonwebtoken::decode_header(token).unwrap();
        assert_eq!(header.typ.as_deref(), Some("at+jwt"));
        let der = STANDARD
            .decode(gateway.secret("VEOVEO_ACCESS_TOKEN_SIGNING_KEY_B64"))
            .unwrap();
        Self {
            key_id: header.kid.unwrap(),
            claims: claims(token),
            key: EncodingKey::from_ed_der(&der),
        }
    }

    fn encode(&self, typ: &str, claims: &Value, key: &EncodingKey) -> String {
        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some(typ.to_owned());
        header.kid = Some(self.key_id.clone());
        bearer(&jsonwebtoken::encode(&header, claims, key).unwrap())
    }

    fn sign(&self, change: impl FnOnce(&mut Value)) -> String {
        let mut claims = self.claims.clone();
        change(&mut claims);
        self.encode("at+jwt", &claims, &self.key)
    }

    fn with_type(&self, typ: &str) -> String {
        self.encode(typ, &self.claims, &self.key)
    }

    fn with_key(&self, key: &EncodingKey) -> String {
        self.encode("at+jwt", &self.claims, key)
    }
}
