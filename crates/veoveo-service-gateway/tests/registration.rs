//! Runs the Gateway binary against a disposable SurrealDB, registers core's modular
//! fixture server through the RFC 7591 flow with core's registration client, and
//! checks the role decisions, the namespace pin and lease expiry from a caller's side.

use std::{
    collections::BTreeSet,
    net::{Ipv4Addr, SocketAddr, TcpListener},
    path::{Path, PathBuf},
    process::Stdio,
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
use tokio::process::{Child, Command};
use veoveo_mcp_contract::{GatewayInternalIdentity, GatewayInternalTokenVerifier};
use veoveo_mcp_ext_registration::{
    AuthorizationDeclaration, ClientMetadata, ModuleDeclaration, RegistrationDeclaration,
    RegistrationErrorCode, ServerEndpoint, ServerVersion,
    client::{ClientError, RegistrationClient},
    statement::ServerKeyPair,
};
use veoveo_modular_fixture_mcp::mcp::{FixtureGrants, ObservatoryMcp};
use veoveo_oauth_client::{CLIENT_ASSERTION_TYPE, ClientAssertionKeyId, ClientAssertionSigner};
use veoveo_service_gateway::local::{LocalCaller, LocalInstallation};
use veoveo_test_fixtures::store::{Server, Storage};
use veoveo_types::{OAuthClientId, ResourceScheme, ServerSlug};

const DATABASE_PASSWORD: &str = "registration-fixture-password";
const READING: &str = "observatory://reading/sensor-a";
const CONTRACT: &str = "observatory://contract";
const DOCUMENTS: &str = "observatory://docs";

#[tokio::test]
async fn registered_servers_route_by_role_keep_their_namespace_and_expire() {
    tokio::time::timeout(Duration::from_secs(180), qualify())
        .await
        .expect("registration qualification exceeded three minutes");
}

async fn qualify() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let (database, endpoint) = Server::start(Storage::Memory, DATABASE_PASSWORD)
        .await
        .unwrap();
    let gateway = Gateway::start(directory.path(), &endpoint).await;
    let http = reqwest::Client::new();

    // The fixture server listens before it registers, so its endpoint is known.
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let server_endpoint = format!("http://{}/observatory/mcp", listener.local_addr().unwrap());
    let issuer = url::Url::parse(&gateway.base).unwrap();
    let registration_endpoint = RegistrationClient::discover(&http, &issuer).await.unwrap();
    let client = registration_client(
        &http,
        &registration_endpoint,
        &gateway.registration_secret,
        &server_endpoint,
        ServerKeyPair::generate().unwrap(),
    );
    let registration = client.register().await.unwrap();
    assert_eq!(registration.access_tokens().issuer.as_str(), gateway.base);
    let database_grant = registration
        .database()
        .expect("a module receives a database user");
    assert!(database_grant.username.as_str().starts_with("observatory_"));

    let fixture = Fixture::default();
    *fixture.verifier.lock().unwrap() = Some(
        GatewayInternalTokenVerifier::from_access_tokens(
            registration.access_tokens(),
            ServerSlug::parse("observatory").unwrap(),
        )
        .unwrap(),
    );
    let served = fixture.clone();
    tokio::spawn(async move {
        let service = StreamableHttpService::new(
            || Ok(ObservatoryMcp),
            veoveo_mcp_contract::stateless_session_manager(),
            veoveo_mcp_contract::canonical_streamable_http_server_config(),
        );
        let router = Router::new()
            .nest_service("/observatory/mcp", service)
            .layer(axum::middleware::from_fn_with_state(served, authenticate));
        axum::serve(listener, router).await.unwrap();
    });
    let stale = registration.clone();
    let maintained = tokio::spawn(async move {
        client
            .maintain(registration, std::future::pending::<()>())
            .await
    });

    // A second key cannot claim the pinned namespace.
    let impostor = registration_client(
        &http,
        &registration_endpoint,
        &gateway.registration_secret,
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

    // Without renewals the lease lapses and the server leaves routing.
    maintained.abort();
    let _ = maintained.await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(read(&admin, READING).await.is_err());
    let lapsed = registration_client(
        &http,
        &registration_endpoint,
        &gateway.registration_secret,
        &server_endpoint,
        ServerKeyPair::generate().unwrap(),
    );
    assert!(matches!(
        lapsed.read(&stale).await,
        Err(ClientError::Rejected {
            status: reqwest::StatusCode::UNAUTHORIZED,
            ..
        })
    ));
    member.cancel().await.unwrap();
    admin.cancel().await.unwrap();
    drop(gateway);
    drop(database);
}

async fn read(
    client: &RunningService<RoleClient, ()>,
    uri: &str,
) -> Result<rmcp::model::ReadResourceResult, rmcp::service::ServiceError> {
    client
        .read_resource(ReadResourceRequestParams::new(uri.to_owned()))
        .await
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
    let module: ModuleDeclaration = serde_json::from_value(serde_json::json!({
        "name": "observatory",
        "ownership": [{ "kind": "table_prefix", "name": "observatory_" }],
        "migrations": [{
            "version": 0,
            "name": "current",
            "sql": "DEFINE TABLE observatory_reading SCHEMAFULL;"
        }]
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
        Some(module),
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
        fixture
            .verifier
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|verifier| verifier.verify(&bearer).ok())
    });
    let Some(identity) = verified else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    fixture.forwarded.lock().unwrap().push(identity);
    request
        .extensions_mut()
        .insert(FixtureGrants(BTreeSet::from([
            veoveo_modular_fixture_mcp::contract::ObservatoryScope::Read.into(),
        ])));
    next.run(request).await
}

/// The Gateway binary, its local installation and its log.
struct Gateway {
    base: String,
    installation: LocalInstallation,
    registration_secret: String,
    log: PathBuf,
    child: Child,
}

impl Gateway {
    async fn start(directory: &Path, database: &str) -> Self {
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let base = format!("http://127.0.0.1:{port}");
        let installation = veoveo_service_gateway::local::generate(directory, &base).unwrap();
        // Short leases let the test observe an expiry.
        let mut config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&installation.config).unwrap()).unwrap();
        config["registration"]["lease"] =
            serde_json::json!({ "renewEverySeconds": 1, "expiresAfterSeconds": 3 });
        let config_path = directory.join("gateway-test.json");
        std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
        let secrets: Vec<(String, String)> = std::fs::read_to_string(&installation.secrets)
            .unwrap()
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect();
        let registration_secret = secrets
            .iter()
            .find(|(name, _)| name == "VEOVEO_REGISTRATION_SECRET")
            .map(|(_, value)| value.clone())
            .unwrap();
        let log = directory.join("gateway.log");
        let output = std::fs::File::create(&log).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_veoveo-service-gateway"))
            .arg("serve")
            .current_dir(directory)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("RUST_LOG", "warn,veoveo_service_gateway=info")
            .env("NO_COLOR", "1")
            .env(
                "VEOVEO_LISTEN_ADDRESS",
                SocketAddr::from((Ipv4Addr::LOCALHOST, port)).to_string(),
            )
            .env("VEOVEO_ISSUER", &base)
            .env("VEOVEO_CONFIG", &config_path)
            .env("VEOVEO_SURREAL_ENDPOINT", database)
            .env("VEOVEO_SURREAL_NAMESPACE", "veoveo")
            .env("VEOVEO_SURREAL_DATABASE", "platform")
            .env("VEOVEO_SURREAL_AUTH_LEVEL", "root")
            .env("VEOVEO_SURREAL_USERNAME", "fixture_admin")
            .env("VEOVEO_SURREAL_PASSWORD", DATABASE_PASSWORD)
            .envs(secrets)
            .stdin(Stdio::null())
            .stdout(output.try_clone().unwrap())
            .stderr(output)
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let gateway = Self {
            base,
            installation,
            registration_secret,
            log,
            child,
        };
        gateway.ready().await;
        gateway
    }

    async fn ready(&self) {
        let http = reqwest::Client::new();
        for _ in 0..300 {
            if let Ok(response) = http.get(format!("{}/readyz", self.base)).send().await
                && response.status() == reqwest::StatusCode::OK
            {
                let live = http
                    .get(format!("{}/livez", self.base))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(live.status(), reqwest::StatusCode::NO_CONTENT);
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!(
            "the Gateway did not become ready:\n{}",
            std::fs::read_to_string(&self.log).unwrap_or_default()
        );
    }

    fn caller(&self, role: &str) -> &LocalCaller {
        self.installation
            .callers
            .iter()
            .find(|caller| caller.role == role)
            .unwrap()
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        if std::thread::panicking() {
            eprintln!(
                "Gateway log:\n{}",
                std::fs::read_to_string(&self.log).unwrap_or_default()
            );
        }
    }
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
        let installation = &gateway.installation;
        let signer = ClientAssertionSigner::from_rsa_pem(
            ClientAssertionKeyId::parse(self.key_id.clone()).unwrap(),
            &std::fs::read(&self.key).unwrap(),
        )
        .unwrap();
        let client = OAuthClientId::parse(self.client_id.clone()).unwrap();
        let token_endpoint = url::Url::parse(&installation.token_endpoint).unwrap();
        let assertion = signer.assertion(&client, &token_endpoint).unwrap();
        let form = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("grant_type", "client_credentials")
            .append_pair("client_id", client.as_str())
            .append_pair("client_assertion_type", CLIENT_ASSERTION_TYPE)
            .append_pair("client_assertion", assertion.expose_secret())
            .append_pair("resource", &installation.resource)
            .append_pair("scope", installation.scope)
            .finish();
        let response = http
            .post(token_endpoint)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(form)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(status, reqwest::StatusCode::OK, "token response: {body}");
        let token = body["access_token"].as_str().unwrap().to_owned();
        ().serve_with_lifecycle(
            StreamableHttpClientTransport::from_config(
                StreamableHttpClientTransportConfig::with_uri(installation.resource.clone())
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
