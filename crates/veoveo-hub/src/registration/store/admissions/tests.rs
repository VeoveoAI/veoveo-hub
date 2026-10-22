use super::*;
use crate::{
    control_plane::{
        HubControlPlaneRevision, HubControlPlaneRevisionId, HubControlPlaneRevisionSource,
    },
    registration::admission::RegistrationAuthority,
    registration::store::{NewRegistration, Pin},
};
use std::collections::BTreeMap;
use veoveo_hub_sdk::{
    DeploymentId,
    registration::{
        AuthorizationDeclaration, RegistrationDeclaration, ServerEndpoint, ServerVersion,
        statement::ServerKeyPair,
    },
};
use veoveo_types::{OAuthClientId, ResourceScheme, ServerSlug};

fn metadata(key: &ServerKeyPair, deployment: DeploymentId) -> ClientMetadata {
    ClientMetadata::new(
        *key.public_key(),
        RegistrationDeclaration {
            deployment,
            slug: ServerSlug::parse("observatory").unwrap(),
            uri_scheme: ResourceScheme::parse("observatory").unwrap(),
            version: ServerVersion::new("0.1.0").unwrap(),
            endpoint: ServerEndpoint::new(
                url::Url::parse(&format!("https://candidate.example/{deployment}/mcp")).unwrap(),
            )
            .unwrap(),
        },
        AuthorizationDeclaration {
            permissions: Vec::new(),
            requirements: Default::default(),
            uses: Vec::new(),
        },
    )
    .unwrap()
}

async fn register(
    store: &RegistrationStore,
    authority: &RegistrationAuthority,
    metadata: &ClientMetadata,
    key: &ServerKeyPair,
) -> (Pin, OAuthClientId) {
    let id = uuid::Uuid::now_v7().simple().to_string();
    let client = OAuthClientId::parse(format!("observatory-{id}")).unwrap();
    let statement = key
        .sign_statement(
            metadata,
            &url::Url::parse("https://hub.example/oauth/register").unwrap(),
            Utc::now(),
        )
        .unwrap();
    let login = veoveo_store::LoginName::parse(format!("observatory_{id}")).unwrap();
    let password = secrecy::SecretString::from("fixture-only-password");
    let token = Sha256Digest::from_bytes([3; 32]);
    let result = store
        .register(NewRegistration {
            authority,
            client_id: &client,
            metadata,
            software_statement: &statement,
            token_digest: &token,
            login: &login,
            password: &password,
            now: Utc::now(),
        })
        .await
        .unwrap();
    (result, client)
}

async fn publish(store: &RegistrationStore, control_plane: HubControlPlane) {
    let hub = crate::store::HubStore::new(store.platform().clone());
    let previous = hub.active_control_revision().await.unwrap();
    let revision = HubControlPlaneRevision {
        revision_id: HubControlPlaneRevisionId::parse(format!("gcp-{}", uuid::Uuid::now_v7()))
            .unwrap(),
        sha256: "0".repeat(64),
        source: HubControlPlaneRevisionSource::SeedFile,
        applied_at: Utc::now(),
        applied_by: PrincipalId::parse("operator").unwrap(),
        tenant: None,
        control_plane,
    };
    hub.publish_control_revision(
        crate::store::ControlPublication {
            revision: &revision,
            replaces: previous.as_ref().map(|previous| &previous.revision_id),
            objects: &[],
        },
        &crate::audit::AuditAppend::new(Default::default()),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn scoped_admission_is_one_use_reissuable_and_never_rebinds_namespace_or_deployment() {
    tokio::time::timeout(std::time::Duration::from_secs(60), qualification())
        .await
        .expect("scoped admission test exceeded one minute");
}

async fn qualification() {
    let db = crate::test_store::TestDb::new().await;
    let store = RegistrationStore::new(db.a.clone());
    let namespace_key = ServerKeyPair::generate().unwrap();
    let base = metadata(&namespace_key, DeploymentId::new());
    assert_eq!(
        register(
            &store,
            &RegistrationAuthority::Namespace,
            &base,
            &namespace_key
        )
        .await
        .0,
        Pin::Claimed
    );
    let claim = store.claim(base.slug()).await.unwrap();
    let worker_key = ServerKeyPair::generate().unwrap();
    let worker = metadata(&worker_key, DeploymentId::new());
    let environment = EnvironmentId::new();
    let other_environment = EnvironmentId::new();
    let mut control: HubControlPlane = serde_json::from_value(serde_json::json!({
        "identityProviders": [], "authorizationServers": [], "profiles": [],
        "tenants": [{ "id": "tenant" }], "workContexts": [], "policies": [], "dataLabels": [], "oidcClients": []
    })).unwrap();
    let owner = EnvironmentOwner {
        tenant: TenantId::parse("tenant").unwrap(),
        principal: PrincipalId::parse("owner").unwrap(),
    };
    let record = crate::execution::EnvironmentRecord {
        owner,
        display_name: "Owned".to_owned(),
        readers: Default::default(),
        base: None,
        components: Default::default(),
    };
    control.execution = Some(crate::execution::ExecutionConfiguration {
        default_environment: environment,
        environments: BTreeMap::from([(environment, record.clone()), (other_environment, record)]),
    });
    let runtime = NativeUpstreamBinding {
        endpoint: worker.registration.endpoint.clone(),
        workspace: "workspace".to_owned(),
        sandbox: "candidate".to_owned(),
        port: 8080,
    };
    control.native_upstreams = Some(crate::control_plane::NativeUpstreams {
        gateway: url::Url::parse("https://gateway.example:8080").unwrap(),
        certificate_authority: "/public/ca.crt".into(),
        bearer_environment: "NATIVE_CREDENTIAL".to_owned(),
        bindings: vec![runtime.clone()],
    });
    publish(&store, control.clone()).await;
    let request = DeploymentAdmissionRequest {
        environment,
        metadata: worker.clone(),
        storage: StorageBinding::SharedLogicalSchema {
            server: worker.slug().clone(),
        },
        runtime,
        expires_at: Utc::now() + chrono::TimeDelta::minutes(2),
    };
    let digest = Sha256Digest::from_bytes([1; 32]);
    store.admit_deployment(&request, &digest).await.unwrap();
    // Every concurrent request holds the same pre-consumption snapshot. The SQL
    // consume/CAS, not that snapshot, arbitrates the one actual registration.
    let mut replies = Vec::new();
    for _ in 0..8 {
        let authority = RegistrationAuthority::Deployment(Box::new(
            store.deployment_admission(&digest).await.unwrap().unwrap(),
        ));
        replies.push(authority);
    }
    let results = futures::future::join_all(
        replies
            .iter()
            .map(|authority| register(&store, authority, &worker, &worker_key)),
    )
    .await;
    assert_eq!(
        results.iter().filter(|(pin, _)| *pin == Pin::Held).count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|(pin, _)| *pin == Pin::AdmissionRejected)
            .count(),
        7
    );
    assert_eq!(store.claim(worker.slug()).await.unwrap(), claim);
    let winner = &results.iter().find(|(pin, _)| *pin == Pin::Held).unwrap().1;
    store.deregister(winner).await.unwrap().unwrap();
    let next = Sha256Digest::from_bytes([2; 32]);
    store.admit_deployment(&request, &next).await.unwrap();
    assert!(store.deployment_admission(&digest).await.unwrap().is_none());
    let authority = RegistrationAuthority::Deployment(Box::new(
        store.deployment_admission(&next).await.unwrap().unwrap(),
    ));
    assert_eq!(
        register(&store, &authority, &worker, &worker_key).await.0,
        Pin::Held
    );
    let mut changed_key = request.clone();
    changed_key.metadata = metadata(
        &ServerKeyPair::generate().unwrap(),
        worker.registration.deployment,
    );
    assert!(
        store
            .admit_deployment(&changed_key, &Sha256Digest::from_bytes([4; 32]))
            .await
            .is_err()
    );
    let mut changed_environment = request.clone();
    changed_environment.environment = other_environment;
    assert!(
        store
            .admit_deployment(&changed_environment, &Sha256Digest::from_bytes([4; 32]))
            .await
            .is_err()
    );
    control
        .execution
        .as_mut()
        .unwrap()
        .environments
        .get_mut(&environment)
        .unwrap()
        .owner
        .principal = PrincipalId::parse("different-owner").unwrap();
    publish(&store, control).await;
    assert!(
        store
            .admit_deployment(&request, &Sha256Digest::from_bytes([4; 32]))
            .await
            .is_err()
    );
    assert_eq!(store.claim(worker.slug()).await.unwrap(), claim);
}
