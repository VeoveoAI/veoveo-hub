//! Runs the Gateway binary against a disposable PostgreSQL database, registers the SDK's
//! Observatory fixture server through the RFC 7591 flow with the SDK's registration client,
//! and checks the role decisions, the namespace pin, the database grant, that a live
//! registration costs no writes while idle, and the end of a registration from the
//! server's and a caller's side.

#[path = "support/gateway.rs"]
mod gateway;

use std::{
    collections::BTreeSet,
    net::Ipv4Addr,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{DateTime, Utc};
use gateway::Gateway;
use rmcp::{
    ClientLifecycleMode, ClientServiceExt,
    model::{ProtocolVersion, ReadResourceRequestParams},
    service::{RoleClient, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
        streamable_http_server::StreamableHttpService,
    },
};
use secrecy::SecretString;
use veoveo_sdk_rust::registration::{
    AccessTokenTrust, AuthorizationDeclaration, ClientMetadata, DatabaseGrant, Ed25519Jwk,
    Ed25519JwkSet, RegistrationDeclaration, RegistrationErrorCode, ServerEndpoint, ServerVersion,
    client::{ClientError, RegistrationClient},
    statement::ServerKeyPair,
};
use veoveo_sdk_rust::{GatewayInternalIdentity, GatewayInternalTokenVerifier};
use veoveo_sdk_rust_fixture::mcp::{FixtureGrants, ObservatoryMcp};
use veoveo_service_gateway::{local::LocalCaller, registration::HOUSEKEEPING_INTERVAL};
use veoveo_store::{Instance, PlatformStore, StoreConfig};
use veoveo_test_fixtures::store::TestDb;
use veoveo_types::{ResourceScheme, ServerSlug};

const READING: &str = "observatory://reading/sensor-a";
const CONTRACT: &str = "observatory://contract";
const DOCUMENTS: &str = "observatory://docs";
/// The grace period the Gateway gives a registration without a live process.
const GRACE_SECONDS: u64 = 5;

#[tokio::test]
async fn registered_servers_route_by_role_receive_a_login_and_end_with_their_process() {
    tokio::time::timeout(Duration::from_secs(180), qualify())
        .await
        .expect("registration qualification exceeded three minutes");
}

async fn qualify() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    // A rotation's next key: published for verification, never used to sign.
    let next_key = ServerKeyPair::generate().unwrap();
    let gateway = Gateway::start_with(
        directory.path(),
        &db,
        Duration::from_secs(GRACE_SECONDS),
        &[(
            "VEOVEO_FORWARDED_TOKEN_VERIFICATION_KEY_B64",
            BASE64_STANDARD.encode(next_key.pkcs8_der()),
        )],
        |_| {},
    )
    .await;
    let signing_key_id = Ed25519Jwk::new(
        *ServerKeyPair::from_pkcs8_der(
            &BASE64_STANDARD
                .decode(gateway.secret("VEOVEO_FORWARDED_TOKEN_KEY_B64"))
                .unwrap(),
        )
        .unwrap()
        .public_key(),
    )
    .kid();
    let registration_secret = gateway.secret("VEOVEO_REGISTRATION_SECRET");
    let http = reqwest::Client::new();
    let slug = ServerSlug::parse("observatory").unwrap();

    // The fixture server listens before it registers, so its endpoint is known.
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let server_endpoint = format!("http://{}/observatory/mcp", listener.local_addr().unwrap());
    let issuer = url::Url::parse(&gateway.base).unwrap();
    let registration_endpoint = RegistrationClient::discover(&http, &issuer).await.unwrap();
    let key = ServerKeyPair::generate().unwrap();
    let replica_key = ServerKeyPair::from_pkcs8_der(key.pkcs8_der()).unwrap();
    let client = registration_client(
        &http,
        &registration_endpoint,
        registration_secret,
        &server_endpoint,
        key,
    );
    let registration = client.register().await.unwrap();
    assert_eq!(registration.access_tokens().issuer.as_str(), gateway.base);

    // The grant is a login in the server's group role: it migrates the server's own
    // schema as that role and holds the kernel instance that keeps it registered.
    let grant = registration
        .database()
        .expect("every registration receives a login")
        .clone();
    assert!(grant.username.as_str().starts_with("observatory_"));
    let server = connect(&grant).await;
    // Loaded at runtime: `sqlx::migrate!` would apply this crate's `sqlx.toml`, which
    // names the Gateway's own history table, to a server's migrations.
    let migrator = sqlx::migrate::Migrator::new(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/registration/migrations"
    )))
    .await
    .unwrap();
    server.migrate_server(&slug, migrator).await.unwrap();
    let access = sqlx::query_file!(
        "tests/registration/table_access.sql",
        grant.username.as_str(),
        "observatory",
        "reading",
    )
    .fetch_one(db.a.pool())
    .await
    .unwrap();
    assert_eq!(access.owner, "server_observatory");
    assert!(access.insert, "the login writes its server's tables");
    assert_eq!(server.current_servers().await.unwrap(), ["observatory"]);
    let instance = Instance::start(&server, &slug).await.unwrap();

    // The issuer's RFC 8414 `jwks_uri` publishes the keys the registration response
    // names, so the fixture verifies every forwarded token with the published set.
    let published = published_keys(&http, &gateway).await;
    assert_eq!(published, registration.access_tokens().jwks);
    assert_eq!(
        serde_json::to_value(&published).unwrap()["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| key["kid"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        [
            signing_key_id.as_str().to_owned(),
            Ed25519Jwk::new(*next_key.public_key())
                .kid()
                .as_str()
                .to_owned()
        ],
        "the set publishes the signing key, then the verification key"
    );
    let fixture = Fixture::default();
    *fixture.verifier.lock().unwrap() = Some(
        GatewayInternalTokenVerifier::from_access_tokens(
            &AccessTokenTrust {
                jwks: published,
                ..registration.access_tokens().clone()
            },
            slug.clone(),
        )
        .unwrap(),
    );
    let served = fixture.clone();
    tokio::spawn(async move {
        let service = StreamableHttpService::new(
            || Ok(ObservatoryMcp),
            veoveo_sdk_rust::stateless_session_manager(),
            veoveo_sdk_rust::canonical_streamable_http_server_config(),
        );
        let router = Router::new()
            .nest_service("/observatory/mcp", service)
            .layer(axum::middleware::from_fn_with_state(served, authenticate));
        axum::serve(listener, router).await.unwrap();
    });

    // A second key cannot claim the pinned namespace.
    let impostor = registration_client(
        &http,
        &registration_endpoint,
        registration_secret,
        &server_endpoint,
        ServerKeyPair::generate().unwrap(),
    );
    match impostor.register().await {
        Err(ClientError::Rejected {
            status: reqwest::StatusCode::BAD_REQUEST,
            error: Some(error),
        }) => assert_eq!(
            error.error,
            RegistrationErrorCode::UnapprovedSoftwareStatement
        ),
        other => panic!("expected a pinned-namespace rejection, got {other:?}"),
    }
    // No server may take the Gateway's own schema as its slug.
    let gateway_slug = gateway_slug_client(
        &http,
        &registration_endpoint,
        registration_secret,
        &server_endpoint,
    );
    match gateway_slug.register().await {
        Err(ClientError::Rejected {
            status: reqwest::StatusCode::BAD_REQUEST,
            error: Some(error),
        }) => assert_eq!(error.error, RegistrationErrorCode::InvalidClientMetadata),
        other => panic!("expected the gateway slug to be refused, got {other:?}"),
    }
    // The installation's registration secret is the initial access token.
    let unauthorized = registration_client(
        &http,
        &registration_endpoint,
        "not-the-registration-secret",
        &server_endpoint,
        ServerKeyPair::generate().unwrap(),
    );
    assert!(matches!(
        unauthorized.register().await,
        Err(ClientError::Rejected {
            status: reqwest::StatusCode::UNAUTHORIZED,
            ..
        })
    ));

    // The member role reads ordinary readings but no sensitive resource; the admin
    // role reads both. Neither reads a resource the server did not declare.
    let member = gateway.caller("member").connect(&http, &gateway).await;
    let admin = gateway.caller("admin").connect(&http, &gateway).await;
    let listed = member.list_all_resources().await.unwrap();
    assert!(
        listed
            .iter()
            .any(|resource| resource.uri == "observatory://readings"),
        "the registered server's resources are listed through the Gateway"
    );
    assert!(read(&member, READING).await.is_ok());
    assert!(read(&member, CONTRACT).await.is_err());
    assert!(read(&admin, CONTRACT).await.is_ok());
    assert!(read(&member, DOCUMENTS).await.is_err());
    assert!(read(&admin, DOCUMENTS).await.is_err());
    assert_eq!(
        *fixture.key_ids.lock().unwrap(),
        BTreeSet::from([signing_key_id.as_str().to_owned()]),
        "every forwarded token is signed by the signing key alone"
    );
    let forwarded = fixture.forwarded.lock().unwrap().clone();
    let member_permissions = forwarded
        .iter()
        .find(|identity| identity.client.as_str() == "acceptance-member")
        .expect("the server saw a member request")
        .permissions
        .iter()
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        member_permissions,
        BTreeSet::from(["observatory.readings.read".to_owned()]),
        "a member's forwarded token carries no sensitive permission"
    );
    assert!(forwarded.iter().any(|identity| {
        identity.client.as_str() == "acceptance-admin"
            && identity
                .permissions
                .iter()
                .any(|permission| permission.as_str() == "observatory.contract.read")
    }));

    // An idle installation writes nothing: once housekeeping has seen the server's
    // instance, its rounds leave the live registration's row version as it is.
    let settled = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let rows = row_versions(&db).await;
            if rows.iter().all(|(_, _, orphaned_at)| orphaned_at.is_none()) {
                return rows;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("housekeeping keeps an orphan mark on a live registration");
    tokio::time::sleep(HOUSEKEEPING_INTERVAL * 3 + Duration::from_secs(1)).await;
    assert_eq!(
        row_versions(&db).await,
        settled,
        "housekeeping wrote a live registration"
    );

    // A second registration of the same server under the pinned key receives its own
    // login, and deregistration drops that login.
    let replica = registration_client(
        &http,
        &registration_endpoint,
        registration_secret,
        &server_endpoint,
        replica_key,
    );
    let second = replica.register().await.unwrap();
    let second_grant = second.database().unwrap().clone();
    assert_ne!(second_grant.username, grant.username);
    connect(&second_grant).await.close().await;
    assert!(role_exists(&db, &second_grant).await);
    replica.deregister(second).await.unwrap();
    assert!(!role_exists(&db, &second_grant).await);
    assert!(
        try_connect(&second_grant).await.is_err(),
        "a dropped login cannot connect"
    );
    assert!(read(&admin, READING).await.is_ok());

    // The process ends without deregistering: its instance's lock is released, and
    // after the grace period the Gateway removes the registration and its login.
    drop(instance);
    server.close().await;
    let removed = tokio::time::timeout(Duration::from_secs(30), async {
        while role_exists(&db, &grant).await {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await;
    assert!(
        removed.is_ok(),
        "the Gateway kept a registration without a live process"
    );
    let routed_out = tokio::time::timeout(Duration::from_secs(10), async {
        while read(&admin, READING).await.is_ok() {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await;
    assert!(routed_out.is_ok(), "an ended registration still routes");
    assert!(matches!(
        client.read(&registration).await,
        Err(ClientError::Rejected {
            status: reqwest::StatusCode::UNAUTHORIZED,
            ..
        })
    ));
    member.cancel().await.unwrap();
    admin.cancel().await.unwrap();
    drop(gateway);
}

/// The forwarded-token keys at the `jwks_uri` of the Gateway issuer's metadata.
async fn published_keys(http: &reqwest::Client, gateway: &Gateway) -> Ed25519JwkSet {
    let metadata: serde_json::Value = http
        .get(format!(
            "{}/.well-known/oauth-authorization-server",
            gateway.base
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    http.get(metadata["jwks_uri"].as_str().unwrap())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

async fn read(
    client: &RunningService<RoleClient, ()>,
    uri: &str,
) -> Result<rmcp::model::ReadResourceResult, rmcp::service::ServiceError> {
    client
        .read_resource(ReadResourceRequestParams::new(uri.to_owned()))
        .await
}

async fn try_connect(grant: &DatabaseGrant) -> Result<PlatformStore, veoveo_store::StoreError> {
    let config = StoreConfig::builder(grant.url.as_str(), grant.password.clone())
        .username(grant.username.as_str())
        .max_connections(2)
        .build()?;
    PlatformStore::connect(config).await
}

async fn connect(grant: &DatabaseGrant) -> PlatformStore {
    try_connect(grant)
        .await
        .expect("the registration grant connects")
}

/// Each registration's client, row version and orphan mark.
async fn row_versions(db: &TestDb) -> Vec<(String, String, Option<DateTime<Utc>>)> {
    sqlx::query_file!("tests/registration/row_versions.sql")
        .fetch_all(db.a.pool())
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.client_id, row.version, row.orphaned_at))
        .collect()
}

async fn role_exists(db: &TestDb, grant: &DatabaseGrant) -> bool {
    sqlx::query_file_scalar!(
        "tests/registration/role_exists.sql",
        grant.username.as_str()
    )
    .fetch_one(db.a.pool())
    .await
    .unwrap()
}

fn registration_client(
    http: &reqwest::Client,
    endpoint: &url::Url,
    secret: &str,
    server_endpoint: &str,
    key: ServerKeyPair,
) -> RegistrationClient {
    let authorization: AuthorizationDeclaration = serde_json::from_value(serde_json::json!({
        "permissions": [
            { "permission": "observatory.readings.read", "resource": "observatory.reading", "access": "read" },
            { "permission": "observatory.contract.read", "resource": "observatory.contract", "access": "read", "risks": ["sensitive"] }
        ],
        "requirements": {
            "resources": {
                "observatory://readings": "observatory.readings.read",
                "observatory://contract": "observatory.contract.read"
            },
            "resourceTemplates": {
                "observatory://reading/{reading_id}": "observatory.readings.read"
            }
        }
    }))
    .unwrap();
    let metadata = ClientMetadata::new(
        *key.public_key(),
        RegistrationDeclaration {
            slug: ServerSlug::parse("observatory").unwrap(),
            uri_scheme: ResourceScheme::parse("observatory").unwrap(),
            version: ServerVersion::new("0.1.0").unwrap(),
            endpoint: ServerEndpoint::new(url::Url::parse(server_endpoint).unwrap()).unwrap(),
        },
        authorization,
    )
    .unwrap();
    RegistrationClient::new(
        http.clone(),
        endpoint.clone(),
        SecretString::from(secret.to_owned()),
        key,
        metadata,
    )
    .unwrap()
}

/// A registration that claims the slug `gateway`, with no permissions of its own.
fn gateway_slug_client(
    http: &reqwest::Client,
    endpoint: &url::Url,
    secret: &str,
    server_endpoint: &str,
) -> RegistrationClient {
    let key = ServerKeyPair::generate().unwrap();
    let authorization: AuthorizationDeclaration =
        serde_json::from_value(serde_json::json!({ "permissions": [], "requirements": {} }))
            .unwrap();
    let metadata = ClientMetadata::new(
        *key.public_key(),
        RegistrationDeclaration {
            slug: ServerSlug::parse("gateway").unwrap(),
            uri_scheme: ResourceScheme::parse("gateway").unwrap(),
            version: ServerVersion::new("0.1.0").unwrap(),
            endpoint: ServerEndpoint::new(url::Url::parse(server_endpoint).unwrap()).unwrap(),
        },
        authorization,
    )
    .unwrap();
    RegistrationClient::new(
        http.clone(),
        endpoint.clone(),
        SecretString::from(secret.to_owned()),
        key,
        metadata,
    )
    .unwrap()
}

/// The fixture server verifies the Gateway's forwarded token and records what it
/// granted. A verified token stands in for the fixture's own read scope.
#[derive(Clone, Default)]
struct Fixture {
    verifier: Arc<Mutex<Option<GatewayInternalTokenVerifier>>>,
    forwarded: Arc<Mutex<Vec<GatewayInternalIdentity>>>,
    /// The `kid` of every verified token.
    key_ids: Arc<Mutex<BTreeSet<String>>>,
}

async fn authenticate(
    State(fixture): State<Fixture>,
    mut request: Request,
    next: Next,
) -> Response {
    let bearer = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_owned);
    let verified = bearer.and_then(|bearer| {
        let identity = fixture
            .verifier
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|verifier| verifier.verify(&bearer).ok())?;
        let key_id = jsonwebtoken::decode_header(&bearer).ok()?.kid?;
        Some((identity, key_id))
    });
    let Some((identity, key_id)) = verified else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    fixture.forwarded.lock().unwrap().push(identity);
    fixture.key_ids.lock().unwrap().insert(key_id);
    request
        .extensions_mut()
        .insert(FixtureGrants(BTreeSet::from([
            veoveo_sdk_rust_fixture::contract::ObservatoryScope::Read.into(),
        ])));
    next.run(request).await
}

trait Connect {
    async fn connect(
        &self,
        http: &reqwest::Client,
        gateway: &Gateway,
    ) -> RunningService<RoleClient, ()>;
}

impl Connect for LocalCaller {
    /// RFC 7523 client credentials, then an MCP session on the local profile.
    async fn connect(
        &self,
        http: &reqwest::Client,
        gateway: &Gateway,
    ) -> RunningService<RoleClient, ()> {
        let token = gateway.access_token(http, self).await;
        ().serve_with_lifecycle(
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
        .unwrap()
    }
}
