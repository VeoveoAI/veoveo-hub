//! Runs the Gateway binary against a disposable PostgreSQL database and checks over
//! HTTP that its token endpoint answers with the error codes of RFC 6749, RFC 7523 and
//! RFC 8707, that its MCP endpoint challenges as RFC 6750 and RFC 9728 define, and that
//! its RFC 8414 and RFC 9728 metadata names each issuer, endpoint and key.

#[path = "support/gateway.rs"]
mod gateway;

use std::time::Duration;

use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use chrono::{TimeDelta, Utc};
use gateway::{DISCOVER, Gateway, TokenForm};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use reqwest::{
    StatusCode,
    header::{CACHE_CONTROL, CONTENT_TYPE, WWW_AUTHENTICATE},
};
use serde_json::{Value, json};
use sha2::Digest as _;
use veoveo_fixtures::store::TestDb;
use veoveo_oauth::{ClientAssertionKeyId, ClientAssertionSigner};
use veoveo_sdk_rust::registration::{Ed25519Jwk, statement::ServerKeyPair};
use veoveo_sdk_rust::{GatewayProfileId, JwtId, TokenIssuer};
use veoveo_service_gateway::GatewayState;
use veoveo_service_gateway::control_plane::GatewayJwtRevocation;
use veoveo_types::OAuthClientId;

const DEADLINE: Duration = Duration::from_secs(120);
/// A client the token endpoint test adds to the installation outside every Work Context.
const UNAFFILIATED: &str = "unaffiliated-caller";

/// One running Gateway. Fields drop in order, so the process ends before its database.
struct Installation {
    gateway: Gateway,
    db: TestDb,
    _directory: tempfile::TempDir,
}

impl Installation {
    async fn start() -> Self {
        Self::configured(|_| {}).await
    }

    /// A Gateway on an installation `configure` changed.
    async fn configured(configure: impl FnOnce(&mut Value)) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let _ = jsonwebtoken::crypto::aws_lc::DEFAULT_PROVIDER.install_default();
        let directory = tempfile::tempdir().unwrap();
        let db = TestDb::new().await;
        let gateway = Gateway::start_with(
            directory.path(),
            &db,
            Duration::from_secs(60),
            &[],
            configure,
        )
        .await;
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
    // A client with the member's key that no Work Context admits.
    let local = Installation::configured(|config| {
        let clients = config["oauthClients"].as_array_mut().unwrap();
        let mut unaffiliated = clients[0].clone();
        unaffiliated["id"] = json!(UNAFFILIATED);
        unaffiliated["displayName"] = json!("Caller outside every Work Context");
        clients.push(unaffiliated);
    })
    .await;
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

    // RFC 6749 §5.2 defines no `access_denied` for the token endpoint: a Work Context the
    // request names and the client may not act in is an invalid request, and a client
    // that may act in no Work Context is unauthorized for the grant.
    expect_oauth_error(
        gateway,
        &http,
        &fresh().with("work_context", "elsewhere"),
        "invalid_request",
    )
    .await;
    let unaffiliated = OAuthClientId::parse(UNAFFILIATED).unwrap();
    let assertion = signer.assertion(&unaffiliated, &endpoint).unwrap();
    expect_oauth_error(
        gateway,
        &http,
        &gateway.client_credentials(&unaffiliated, assertion.expose_secret()),
        "unauthorized_client",
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

    let discovered = gateway.mcp(&http, Some(&bearer(&token)), DISCOVER).await;
    assert_eq!(discovered.status(), StatusCode::OK);

    // RFC 6750 §3.1: a request without bearer credentials receives no error code.
    expect_challenge(gateway, gateway.mcp(&http, None, DISCOVER).await, None).await;
    let basic = format!("Basic {}", STANDARD.encode("member:password"));
    expect_challenge(
        gateway,
        gateway.mcp(&http, Some(&basic), DISCOVER).await,
        None,
    )
    .await;
    expect_challenge(
        gateway,
        gateway
            .mcp(&http, Some("Bearer two tokens"), DISCOVER)
            .await,
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
        let response = gateway.mcp(&http, Some(&authorization), DISCOVER).await;
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
        gateway.mcp(&http, Some(&unscoped), DISCOVER).await,
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
        gateway.mcp(&http, Some(&bearer(&revoked)), DISCOVER).await,
        Some(("invalid_token", StatusCode::UNAUTHORIZED)),
    )
    .await;
    let still_valid = gateway.mcp(&http, Some(&bearer(&token)), DISCOVER).await;
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

#[tokio::test]
async fn metadata_names_each_issuer_its_endpoints_and_keys() {
    tokio::time::timeout(DEADLINE, metadata())
        .await
        .expect("the metadata checks exceeded two minutes");
}

async fn metadata() {
    let local = Installation::start().await;
    let gateway = &local.gateway;
    let http = reqwest::Client::new();
    let base = gateway.base.as_str();
    let resource = url::Url::parse(&gateway.installation.resource).unwrap();

    // RFC 9728 §3: the protected resource's metadata lives at the well-known URI formed
    // from its identifier and names that identifier (§3.3).
    let protected = document(
        &http,
        &well_known(&resource, "oauth-protected-resource"),
        "application/json",
    )
    .await;
    assert_eq!(protected["resource"], resource.as_str());
    assert_eq!(protected["bearer_methods_supported"], json!(["header"]));
    assert!(
        protected["scopes_supported"]
            .as_array()
            .unwrap()
            .contains(&json!(gateway.installation.scope))
    );
    let issuer = format!("{base}/oauth");
    assert_eq!(protected["authorization_servers"], json!([issuer]));

    // RFC 8414 §3: the authorization server's metadata lives at the well-known URI
    // formed from its issuer and names that issuer (§3.3).
    let server = document(
        &http,
        &well_known(
            &url::Url::parse(&issuer).unwrap(),
            "oauth-authorization-server",
        ),
        "application/json",
    )
    .await;
    assert_eq!(server["issuer"], issuer.as_str());
    assert_eq!(
        server["token_endpoint"],
        gateway.installation.token_endpoint
    );
    assert_eq!(
        server["grant_types_supported"],
        json!(["client_credentials"])
    );
    assert_eq!(
        server["token_endpoint_auth_methods_supported"],
        json!(["private_key_jwt"])
    );
    assert!(
        server["token_endpoint_auth_signing_alg_values_supported"]
            .as_array()
            .unwrap()
            .contains(&json!("RS256"))
    );
    // §2 requires the member; no grant of this server uses an authorization endpoint.
    assert_eq!(server["response_types_supported"], json!([]));
    assert!(server.get("authorization_endpoint").is_none());
    // No client of the local installation holds a refresh token, so none revokes one.
    assert!(server.get("revocation_endpoint").is_none());
    assert!(server.get("registration_endpoint").is_none());

    // `jwks_uri` holds the public key that verifies the tokens this server issues.
    let keys = document(
        &http,
        server["jwks_uri"].as_str().unwrap(),
        veoveo_service_gateway::registration::JWK_SET_MEDIA_TYPE,
    )
    .await;
    let token = gateway.access_token(&http, gateway.caller("member")).await;
    let key_id = jsonwebtoken::decode_header(&token).unwrap().kid.unwrap();
    let jwk = public_ed25519_keys(&keys)
        .into_iter()
        .find(|jwk| jwk["kid"] == key_id.as_str())
        .expect("the token's key is published");
    let mut validation = jsonwebtoken::Validation::new(Algorithm::EdDSA);
    validation.set_issuer(&[issuer.as_str()]);
    validation.set_audience(&[resource.as_str()]);
    jsonwebtoken::decode::<Value>(
        &token,
        &jsonwebtoken::DecodingKey::from_jwk(&serde_json::from_value(jwk).unwrap()).unwrap(),
        &validation,
    )
    .unwrap();

    // The Gateway's own issuer: registration, the token exchange of calls between
    // servers, and the keys of the tokens it forwards.
    let own = document(
        &http,
        &format!("{base}/.well-known/oauth-authorization-server"),
        "application/json",
    )
    .await;
    assert_eq!(own["issuer"], base);
    assert_eq!(
        own["registration_endpoint"],
        format!("{base}/oauth/register")
    );
    assert_eq!(own["token_endpoint"], format!("{base}/oauth/token"));
    assert_eq!(own["response_types_supported"], json!([]));
    assert_eq!(
        own["grant_types_supported"],
        json!(["urn:ietf:params:oauth:grant-type:token-exchange"])
    );
    assert_eq!(
        own["token_endpoint_auth_methods_supported"],
        json!(["private_key_jwt"])
    );
    assert_eq!(
        own["token_endpoint_auth_signing_alg_values_supported"],
        json!(["EdDSA"])
    );
    assert_eq!(own["jwks_uri"], format!("{base}/jwks.json"));
    let forwarded = document(
        &http,
        own["jwks_uri"].as_str().unwrap(),
        veoveo_service_gateway::registration::JWK_SET_MEDIA_TYPE,
    )
    .await;
    let signing = ServerKeyPair::from_pkcs8_der(
        &STANDARD
            .decode(gateway.secret("VEOVEO_FORWARDED_TOKEN_KEY_B64"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        public_ed25519_keys(&forwarded),
        [json!(Ed25519Jwk::new(*signing.public_key()))]
    );

    // RFC 7591 §3 with RFC 6750 §3: registration takes the registration secret as its
    // initial access token.
    let refused = http
        .post(own["registration_endpoint"].as_str().unwrap())
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(refused.headers()[WWW_AUTHENTICATE], "Bearer");
}

/// RFC 8414 §3.1 and RFC 9728 §3.1: the well-known URI string goes between the host and
/// the path of the identifier.
fn well_known(identifier: &url::Url, name: &str) -> String {
    let mut url = identifier.clone();
    url.set_path(&format!(
        "/.well-known/{name}{}",
        identifier.path().trim_end_matches('/')
    ));
    url.to_string()
}

/// A JSON document served with HTTP 200 and `media_type`.
async fn document(http: &reqwest::Client, url: &str, media_type: &str) -> Value {
    let response = http.get(url).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{url}");
    assert_eq!(response.headers()[CONTENT_TYPE], media_type, "{url}");
    response.json().await.unwrap()
}

/// The keys of an RFC 7517 JWK Set, each a public Ed25519 signing key whose `kid` is its
/// RFC 7638 thumbprint.
fn public_ed25519_keys(set: &Value) -> Vec<Value> {
    let keys = set["keys"].as_array().unwrap().clone();
    assert!(!keys.is_empty());
    for key in &keys {
        assert_eq!(key["kty"], "OKP");
        assert_eq!(key["crv"], "Ed25519");
        assert_eq!(key["alg"], "EdDSA");
        assert_eq!(key["use"], "sig");
        assert!(key.get("d").is_none(), "a JWK Set publishes no private key");
        // RFC 7638 §3.2: the required members in lexicographic order, without spaces.
        let canonical = format!(
            r#"{{"crv":"Ed25519","kty":"OKP","x":"{}"}}"#,
            key["x"].as_str().unwrap()
        );
        let thumbprint = URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(canonical.as_bytes()));
        assert_eq!(key["kid"], thumbprint.as_str());
    }
    keys
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
