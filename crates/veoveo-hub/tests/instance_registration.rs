//! Runs the Hub binary against a disposable PostgreSQL database and registers the
//! SDK's Observatory fixture declaration as instances with the SDK's registration
//! client, each presenting the registration token the Hub created for its instance.
//! Checks that two instances of one slug register side by side under different keys,
//! that a registration replaces its instance's previous one with a new key and new
//! metadata under the same deployment, that a token registers only its own
//! deployment, that every other token still takes the namespace path, and that
//! housekeeping never orphans or collects an instance registration. RFC 7592 updates
//! that change the metadata, and namespace registrations of an existing deployment
//! with new metadata, replace it, while the namespace's key pin still holds.

#[path = "support/hub.rs"]
mod hub;

use std::{collections::BTreeSet, time::Duration};

use chrono::{DateTime, Utc};
use hub::Hub;
use secrecy::{ExposeSecret, SecretString};
use veoveo_fixtures::store::TestDb;
use veoveo_hub::data_environment::DatabaseName;
use veoveo_hub::execution::BranchName;
use veoveo_hub::registration::{HOUSEKEEPING_INTERVAL, NewInstance, create_instance};
use veoveo_hub_sdk::DeploymentId;
use veoveo_hub_sdk::registration::{
    AuthorizationDeclaration, ClientInformation, ClientMetadata, ClientUpdateRequest,
    DatabaseGrant, RegistrationDeclaration, RegistrationErrorCode, RegistrationErrorResponse,
    ServerEndpoint, ServerVersion,
    client::{ClientError, Registration, RegistrationClient},
    statement::ServerKeyPair,
};
use veoveo_types::{ResourceScheme, ServerSlug};

/// How long one of these tests may take, Hub start included.
const BOUND: Duration = Duration::from_secs(120);

#[tokio::test]
async fn instances_of_one_slug_register_side_by_side_under_different_keys() {
    tokio::time::timeout(BOUND, async {
        let fixture = Fixture::start(Duration::from_secs(60)).await;
        let a = create_instance(&fixture.db.a, &fixture.installation(), &branch("a"))
            .await
            .unwrap();
        let b = create_instance(&fixture.db.a, &fixture.installation(), &branch("b"))
            .await
            .unwrap();
        assert_ne!(a.deployment, b.deployment);

        let first = fixture.instance(&a, a.deployment, Revision::First);
        let second = fixture.instance(&b, b.deployment, Revision::First);
        assert_ne!(first.metadata().key(), second.metadata().key());
        let first = first.register().await.unwrap();
        let second = second.register().await.unwrap();
        assert_eq!(deployment(&first), a.deployment);
        assert_eq!(deployment(&second), b.deployment);
        assert_ne!(first.client_id(), second.client_id());
        assert_ne!(grant(&first).username, grant(&second).username);

        let rows = registrations(&fixture.db).await;
        assert_eq!(
            rows.iter()
                .map(|row| (row.client_id.as_str(), row.deployment))
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                (first.client_id().as_str(), a.deployment),
                (second.client_id().as_str(), b.deployment),
            ])
        );
        assert_eq!(
            fixture.routed().await,
            BTreeSet::from([
                ("observatory".to_owned(), a.deployment),
                ("observatory".to_owned(), b.deployment),
            ]),
            "the Hub routes both instances of the slug"
        );
    })
    .await
    .expect("the test exceeded its bound");
}

#[tokio::test]
async fn an_instance_registration_replaces_the_previous_one_under_the_same_deployment() {
    tokio::time::timeout(BOUND, async {
        let fixture = Fixture::start(Duration::from_secs(60)).await;
        let a = create_instance(&fixture.db.a, &fixture.installation(), &branch("a"))
            .await
            .unwrap();
        let b = create_instance(&fixture.db.a, &fixture.installation(), &branch("b"))
            .await
            .unwrap();
        let before = fixture.instance(&a, a.deployment, Revision::First);
        let previous = before.register().await.unwrap();
        let other = fixture
            .instance(&b, b.deployment, Revision::First)
            .register()
            .await
            .unwrap();
        connect(grant(&previous)).await.close().await;

        // A rebuilt branch: a new key, version, endpoint and declaration.
        let after = fixture.instance(&a, a.deployment, Revision::Second);
        assert_ne!(after.metadata().key(), before.metadata().key());
        let current = after.register().await.unwrap();
        assert_eq!(deployment(&current), a.deployment);
        assert_ne!(current.client_id(), previous.client_id());
        assert_eq!(&current.information().metadata, after.metadata());

        let rows = registrations(&fixture.db).await;
        assert_eq!(
            rows.iter()
                .map(|row| (row.client_id.as_str(), row.deployment))
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                (current.client_id().as_str(), a.deployment),
                (other.client_id().as_str(), b.deployment),
            ]),
            "the previous registration of the instance is gone and the other instance stays"
        );
        assert!(!role_exists(&fixture.db, grant(&previous)).await);
        assert!(
            try_connect(grant(&previous)).await.is_err(),
            "the replaced login cannot connect"
        );
        assert!(role_exists(&fixture.db, grant(&current)).await);
        connect(grant(&current)).await.close().await;
        let stored = stored_deployment(&fixture.db, a.deployment).await;
        assert_eq!(stored.0, "observatory");
        assert_eq!(
            &stored.1,
            after.metadata(),
            "the deployment holds the new metadata"
        );
        assert!(matches!(
            before.read(&previous).await,
            Err(ClientError::Rejected {
                status: reqwest::StatusCode::UNAUTHORIZED,
                ..
            })
        ));
        assert_eq!(
            after.read(&current).await.unwrap().client_id(),
            current.client_id()
        );
        assert_eq!(
            fixture.routed().await,
            BTreeSet::from([
                ("observatory".to_owned(), a.deployment),
                ("observatory".to_owned(), b.deployment),
            ])
        );
    })
    .await
    .expect("the test exceeded its bound");
}

#[tokio::test]
async fn an_instance_token_registers_only_its_deployment_and_other_tokens_take_the_namespace_path()
{
    tokio::time::timeout(BOUND, async {
        let fixture = Fixture::start(Duration::from_secs(60)).await;
        let a = create_instance(&fixture.db.a, &fixture.installation(), &branch("a"))
            .await
            .unwrap();
        let b = create_instance(&fixture.db.a, &fixture.installation(), &branch("b"))
            .await
            .unwrap();
        let registered = fixture
            .instance(&a, a.deployment, Revision::First)
            .register()
            .await
            .unwrap();

        // The instance token names the instance's deployment, and no other.
        for named in [b.deployment, DeploymentId::new()] {
            match fixture
                .instance(&a, named, Revision::First)
                .register()
                .await
            {
                Err(ClientError::Rejected {
                    status: reqwest::StatusCode::BAD_REQUEST,
                    error: Some(error),
                }) => assert_eq!(error.error, RegistrationErrorCode::InvalidClientMetadata),
                other => panic!("expected another deployment to be refused, got {other:?}"),
            }
        }
        assert_eq!(
            registrations(&fixture.db)
                .await
                .iter()
                .map(|row| row.client_id.clone())
                .collect::<Vec<_>>(),
            [registered.client_id().to_string()]
        );

        // A token that names no instance is checked as the registration secret.
        let unknown = SecretString::from("not-an-instance-token".to_owned());
        assert!(matches!(
            fixture
                .client(&unknown, DeploymentId::new(), Revision::First)
                .register()
                .await,
            Err(ClientError::Rejected {
                status: reqwest::StatusCode::UNAUTHORIZED,
                ..
            })
        ));
        let secret = fixture.registration_secret();
        let namespace = fixture
            .client(&secret, DeploymentId::new(), Revision::First)
            .register()
            .await
            .unwrap();
        match fixture
            .client(&secret, DeploymentId::new(), Revision::First)
            .register()
            .await
        {
            Err(ClientError::Rejected {
                status: reqwest::StatusCode::BAD_REQUEST,
                error: Some(error),
            }) => assert_eq!(
                error.error,
                RegistrationErrorCode::UnapprovedSoftwareStatement
            ),
            other => panic!("expected the namespace pin to hold, got {other:?}"),
        }

        // The namespace pin binds the namespace path only: the instance registers
        // again under yet another key.
        let replaced = fixture
            .instance(&a, a.deployment, Revision::Second)
            .register()
            .await
            .unwrap();
        assert_eq!(
            registrations(&fixture.db)
                .await
                .iter()
                .map(|row| row.client_id.clone())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                namespace.client_id().to_string(),
                replaced.client_id().to_string()
            ])
        );
    })
    .await
    .expect("the test exceeded its bound");
}

#[tokio::test]
async fn housekeeping_never_orphans_or_collects_an_instance_registration() {
    tokio::time::timeout(BOUND, async {
        let fixture = Fixture::start(Duration::from_secs(1)).await;
        let a = create_instance(&fixture.db.a, &fixture.installation(), &branch("a"))
            .await
            .unwrap();
        let instance = fixture
            .instance(&a, a.deployment, Revision::First)
            .register()
            .await
            .unwrap();
        // A namespace registration without a process shows housekeeping running.
        let secret = fixture.registration_secret();
        let namespace = fixture
            .client(&secret, DeploymentId::new(), Revision::First)
            .register()
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(30), async {
            while role_exists(&fixture.db, grant(&namespace)).await {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
        .await
        .expect("housekeeping kept a namespace registration without a process");
        // Two more rounds past the grace period.
        tokio::time::sleep(HOUSEKEEPING_INTERVAL * 2 + Duration::from_secs(1)).await;

        let login = &grant(&instance).username;
        assert!(
            !alive(&fixture.db, login.as_str()).await,
            "the instance's login holds no live kernel instance"
        );
        let rows = registrations(&fixture.db).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].client_id, instance.client_id().as_str());
        assert_eq!(rows[0].login, login.as_str());
        assert_eq!(
            rows[0].orphaned_at, None,
            "an instance registration is never orphaned"
        );
        assert!(role_exists(&fixture.db, grant(&instance)).await);
        connect(grant(&instance)).await.close().await;
    })
    .await
    .expect("the test exceeded its bound");
}

#[tokio::test]
async fn an_update_with_new_metadata_replaces_the_instance_registration_and_its_deployment() {
    tokio::time::timeout(BOUND, async {
        let fixture = Fixture::start(Duration::from_secs(60)).await;
        let a = create_instance(&fixture.db.a, &fixture.installation(), &branch("a"))
            .await
            .unwrap();
        let registered = fixture
            .instance(&a, a.deployment, Revision::First)
            .register()
            .await
            .unwrap();

        // A rebuilt branch updates in place: a new key, version, endpoint and
        // declaration under the same registration and deployment.
        let key = ServerKeyPair::generate().unwrap();
        let changed = fixture.metadata(&key, a.deployment, Revision::Second);
        let response = fixture.update(&registered, &key, &changed).await;
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let information: ClientInformation = response.json().await.unwrap();
        assert_eq!(information.client_id, *registered.client_id());
        assert_eq!(information.metadata, changed);
        assert_eq!(fixture.read(&registered).await.metadata, changed);
        assert_eq!(
            stored_deployment(&fixture.db, a.deployment).await.1,
            changed
        );
        assert!(
            role_exists(&fixture.db, grant(&registered)).await,
            "the registration keeps its login"
        );

        // An update still keeps the registration's deployment and slug.
        let elsewhere = fixture.metadata(&key, DeploymentId::new(), Revision::Second);
        let refused = fixture.update(&registered, &key, &elsewhere).await;
        assert_eq!(
            rejection(refused).await,
            RegistrationErrorCode::InvalidClientMetadata
        );
        assert_eq!(
            stored_deployment(&fixture.db, a.deployment).await.1,
            changed
        );
    })
    .await
    .expect("the test exceeded its bound");
}

#[tokio::test]
async fn a_namespace_registration_with_new_metadata_replaces_its_deployment_under_the_pinned_key() {
    tokio::time::timeout(BOUND, async {
        let fixture = Fixture::start(Duration::from_secs(60)).await;
        let secret = fixture.registration_secret();
        let key = ServerKeyPair::generate().unwrap();
        let deployment = DeploymentId::new();
        let first = fixture
            .client_with(&secret, copy(&key), deployment, Revision::First)
            .register()
            .await
            .unwrap();
        connect(grant(&first)).await.close().await;

        // The same deployment with other declarations replaces the registration.
        let second = fixture
            .client_with(&secret, copy(&key), deployment, Revision::Second)
            .register()
            .await
            .unwrap();
        assert_ne!(second.client_id(), first.client_id());
        assert_eq!(
            registrations(&fixture.db)
                .await
                .iter()
                .map(|row| (row.client_id.clone(), row.deployment))
                .collect::<Vec<_>>(),
            [(second.client_id().to_string(), deployment)]
        );
        assert!(!role_exists(&fixture.db, grant(&first)).await);
        assert!(try_connect(grant(&first)).await.is_err());
        assert_eq!(
            &stored_deployment(&fixture.db, deployment).await.1,
            &second.information().metadata
        );

        // An update with new metadata under the pinned key applies too.
        let updated = fixture.metadata(&key, deployment, Revision::First);
        let response = fixture.update(&second, &key, &updated).await;
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(stored_deployment(&fixture.db, deployment).await.1, updated);

        // Another key neither registers the claimed slug, for this deployment or any
        // other, nor updates its registration.
        for named in [deployment, DeploymentId::new()] {
            match fixture
                .client(&secret, named, Revision::Second)
                .register()
                .await
            {
                Err(ClientError::Rejected {
                    status: reqwest::StatusCode::BAD_REQUEST,
                    error: Some(error),
                }) => assert_eq!(
                    error.error,
                    RegistrationErrorCode::UnapprovedSoftwareStatement
                ),
                other => panic!("expected the namespace pin to hold, got {other:?}"),
            }
        }
        let other = ServerKeyPair::generate().unwrap();
        let foreign = fixture.metadata(&other, deployment, Revision::Second);
        assert_eq!(
            rejection(fixture.update(&second, &other, &foreign).await).await,
            RegistrationErrorCode::UnapprovedSoftwareStatement
        );
        assert_eq!(stored_deployment(&fixture.db, deployment).await.1, updated);
        assert_eq!(registrations(&fixture.db).await.len(), 1);
    })
    .await
    .expect("the test exceeded its bound");
}

/// The Hub on a fresh database, and the client its registration endpoint is
/// discovered with.
struct Fixture {
    db: TestDb,
    hub: Hub,
    http: reqwest::Client,
    endpoint: url::Url,
    _directory: tempfile::TempDir,
}

impl Fixture {
    async fn start(grace: Duration) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let directory = tempfile::tempdir().unwrap();
        let db = TestDb::new().await;
        let hub = Hub::start(directory.path(), &db, grace).await;
        let http = reqwest::Client::new();
        let endpoint = RegistrationClient::discover(&http, &url::Url::parse(&hub.base).unwrap())
            .await
            .unwrap();
        Self {
            db,
            hub,
            http,
            endpoint,
            _directory: directory,
        }
    }

    /// The installation database, which every instance of these tests registers into.
    fn installation(&self) -> DatabaseName {
        DatabaseName::installation(&self.db.a).unwrap()
    }

    /// The installation's registration secret, the namespace path's initial access
    /// token.
    fn registration_secret(&self) -> SecretString {
        SecretString::from(self.hub.secret("VEOVEO_REGISTRATION_SECRET").to_owned())
    }

    /// A process of `instance` whose metadata names `deployment`.
    fn instance(
        &self,
        instance: &NewInstance,
        deployment: DeploymentId,
        revision: Revision,
    ) -> RegistrationClient {
        self.client(&instance.registration_token, deployment, revision)
    }

    /// A process with a new key presenting `token` as its initial access token.
    fn client(
        &self,
        token: &SecretString,
        deployment: DeploymentId,
        revision: Revision,
    ) -> RegistrationClient {
        self.client_with(
            token,
            ServerKeyPair::generate().unwrap(),
            deployment,
            revision,
        )
    }

    /// A process with `key` presenting `token` as its initial access token.
    fn client_with(
        &self,
        token: &SecretString,
        key: ServerKeyPair,
        deployment: DeploymentId,
        revision: Revision,
    ) -> RegistrationClient {
        let metadata = self.metadata(&key, deployment, revision);
        RegistrationClient::new(
            self.http.clone(),
            self.endpoint.clone(),
            SecretString::from(token.expose_secret().to_owned()),
            key,
            metadata,
        )
        .unwrap()
    }

    /// The Observatory declaration of `revision` under `key` for `deployment`.
    fn metadata(
        &self,
        key: &ServerKeyPair,
        deployment: DeploymentId,
        revision: Revision,
    ) -> ClientMetadata {
        ClientMetadata::new(
            *key.public_key(),
            RegistrationDeclaration {
                deployment,
                slug: ServerSlug::parse("observatory").unwrap(),
                uri_scheme: ResourceScheme::parse("observatory").unwrap(),
                version: ServerVersion::new(revision.version()).unwrap(),
                endpoint: ServerEndpoint::new(
                    url::Url::parse(&format!(
                        "http://127.0.0.1:{}/observatory/mcp",
                        revision.port()
                    ))
                    .unwrap(),
                )
                .unwrap(),
            },
            revision.authorization(),
        )
        .unwrap()
    }

    /// RFC 7592 §2.2: replace `registration`'s metadata with `metadata`, signed by `key`.
    /// The SDK's client updates only to the metadata it registered, so the test sends
    /// the request itself.
    async fn update(
        &self,
        registration: &Registration,
        key: &ServerKeyPair,
        metadata: &ClientMetadata,
    ) -> reqwest::Response {
        let information = registration.information();
        let uri = &information.registration_client_uri;
        let request = ClientUpdateRequest {
            client_id: information.client_id.clone(),
            metadata: metadata.clone(),
            software_statement: key.sign_statement(metadata, uri, Utc::now()).unwrap(),
        };
        self.http
            .put(uri.clone())
            .bearer_auth(information.registration_access_token.expose_secret())
            .json(&request)
            .send()
            .await
            .unwrap()
    }

    /// RFC 7592 §2.1: the registration as the Hub holds it now.
    async fn read(&self, registration: &Registration) -> ClientInformation {
        let information = registration.information();
        let response = self
            .http
            .get(information.registration_client_uri.clone())
            .bearer_auth(information.registration_access_token.expose_secret())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        response.json().await.unwrap()
    }

    /// The slug and deployment of every server the Hub routes, from its server-health
    /// report.
    async fn routed(&self) -> BTreeSet<(String, DeploymentId)> {
        let token = self
            .hub
            .access_token(&self.http, self.hub.caller("admin"))
            .await;
        let response = self
            .http
            .get(format!("{}/admin/local/server-health", self.hub.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let report: serde_json::Value = response.json().await.unwrap();
        report["servers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|server| {
                (
                    server["server"].as_str().unwrap().to_owned(),
                    serde_json::from_value(server["deployment"].clone()).unwrap(),
                )
            })
            .collect()
    }
}

/// Two builds of the Observatory fixture: the second has another version and endpoint
/// and also declares its sensitive contract resource.
#[derive(Clone, Copy)]
enum Revision {
    First,
    Second,
}

impl Revision {
    fn version(self) -> &'static str {
        match self {
            Self::First => "0.1.0",
            Self::Second => "0.2.0",
        }
    }

    /// A loopback port nothing listens on: registration never calls the endpoint.
    fn port(self) -> u16 {
        match self {
            Self::First => 9,
            Self::Second => 19,
        }
    }

    fn authorization(self) -> AuthorizationDeclaration {
        let mut declaration = serde_json::json!({
            "permissions": [
                { "permission": "observatory.readings.read", "resource": "observatory.reading", "access": "read" }
            ],
            "requirements": {
                "resources": {
                    "observatory://readings": {"kind":"permission", "permission":"observatory.readings.read"}
                },
                "resourceTemplates": {
                    "observatory://reading/{reading_id}": {"kind":"permission", "permission":"observatory.readings.read"}
                }
            }
        });
        if let Self::Second = self {
            declaration["permissions"].as_array_mut().unwrap().push(serde_json::json!(
                { "permission": "observatory.contract.read", "resource": "observatory.contract", "access": "read", "risks": ["sensitive"] }
            ));
            declaration["requirements"]["resources"]["observatory://contract"] =
                serde_json::json!({"kind":"permission", "permission":"observatory.contract.read"});
        }
        serde_json::from_value(declaration).unwrap()
    }
}

/// The RFC 7591 error code of a refused request.
async fn rejection(response: reqwest::Response) -> RegistrationErrorCode {
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    response
        .json::<RegistrationErrorResponse>()
        .await
        .unwrap()
        .error
}

fn copy(key: &ServerKeyPair) -> ServerKeyPair {
    ServerKeyPair::from_pkcs8_der(key.pkcs8_der()).unwrap()
}

fn branch(name: &str) -> BranchName {
    BranchName::parse(name).unwrap()
}

fn deployment(registration: &Registration) -> DeploymentId {
    registration.information().metadata.registration.deployment
}

fn grant(registration: &Registration) -> &DatabaseGrant {
    registration
        .database()
        .expect("every registration receives a login")
}

/// One registration row.
struct Row {
    client_id: String,
    deployment: DeploymentId,
    login: String,
    orphaned_at: Option<DateTime<Utc>>,
}

async fn registrations(db: &TestDb) -> Vec<Row> {
    sqlx::query_file!("tests/instance_registration/registrations.sql")
        .fetch_all(db.a.pool())
        .await
        .unwrap()
        .into_iter()
        .map(|row| Row {
            client_id: row.client_id,
            deployment: DeploymentId::try_from(row.deployment).unwrap(),
            login: row.login,
            orphaned_at: row.orphaned_at,
        })
        .collect()
}

/// A deployment's server and stored metadata.
async fn stored_deployment(db: &TestDb, deployment: DeploymentId) -> (String, ClientMetadata) {
    let row = sqlx::query_file!(
        "tests/instance_registration/deployment.sql",
        deployment.as_uuid()
    )
    .fetch_one(db.a.pool())
    .await
    .unwrap();
    (row.server, serde_json::from_value(row.metadata).unwrap())
}

/// Whether `login` holds a live kernel instance.
async fn alive(db: &TestDb, login: &str) -> bool {
    sqlx::query_file_scalar!("tests/instance_registration/alive.sql", login)
        .fetch_one(db.a.pool())
        .await
        .unwrap()
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

async fn try_connect(
    grant: &DatabaseGrant,
) -> Result<veoveo_store::PlatformStore, veoveo_store::StoreError> {
    let config = veoveo_store::StoreConfig::builder(grant.url.as_str(), grant.password.clone())
        .username(grant.username.as_str())
        .max_connections(1)
        .build()?;
    veoveo_store::PlatformStore::connect(config).await
}

async fn connect(grant: &DatabaseGrant) -> veoveo_store::PlatformStore {
    try_connect(grant)
        .await
        .expect("the registration grant connects")
}
