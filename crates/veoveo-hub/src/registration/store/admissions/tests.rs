use super::*;
use crate::activation::{AcceptedApply, BuildState};
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
async fn scoped_admission_reissue_retains_workers_but_never_bypasses_identity_or_withdrawal() {
    tokio::time::timeout(std::time::Duration::from_secs(60), qualification())
        .await
        .expect("scoped admission test exceeded one minute");
}

async fn qualification() {
    let db = crate::test_store::TestDb::new().await;
    let store = RegistrationStore::new(db.a.clone());
    let caller = crate::mcp::task_ownership_tests::subject();
    let hub = crate::store::HubStore::new(db.a.clone());
    crate::test_catalog::publish_work_context(
        &hub,
        &caller.authority.tenant,
        &caller.authority.work_context,
    )
    .await;
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
    let mut control = hub
        .active_control_revision()
        .await
        .unwrap()
        .unwrap()
        .control_plane;
    let owner = EnvironmentOwner {
        tenant: caller.authority.tenant.clone(),
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
        runtime_templates: Default::default(),
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
    let (pin, resumed_client) = register(&store, &authority, &worker, &worker_key).await;
    assert_eq!(pin, Pin::Held);
    store.deregister(&resumed_client).await.unwrap().unwrap();

    // Removing an owned override does not retire a canonical Task's worker. A
    // fresh narrow admission is allowed only while its retained Task and the
    // separate operator eligibility/native binding still authorize that worker.
    use crate::activation::{
        ActivationKey, AdmittedRuntime, ApplyIdempotencyKey, BuildInputs, Generation,
        RuntimeEligibility, RuntimeTemplateAdmission, RuntimeTemplateRef,
    };
    let reference = RuntimeTemplateRef::parse("fixture-runtime-v1").unwrap();
    let admission = RuntimeTemplateAdmission {
        workspace: "workspace".into(),
        template: "fixture-template-v1".into(),
        policy_file: "/etc/fixture-policy.yaml".into(),
        providers: vec![],
        eligible: vec![RuntimeEligibility {
            environment,
            component: worker.slug().clone(),
            storage: request.storage.clone(),
        }],
    };
    control
        .execution
        .as_mut()
        .unwrap()
        .runtime_templates
        .insert(reference.clone(), admission.clone());
    control
        .execution
        .as_mut()
        .unwrap()
        .environments
        .get_mut(&environment)
        .unwrap()
        .components
        .insert(
            worker.slug().clone(),
            veoveo_hub_sdk::ResolvedComponent::Deployment {
                deployment: worker.registration.deployment,
            },
        );
    publish(&store, control.clone()).await;
    let accepted = AcceptedApply {
        runtime: AdmittedRuntime::capture(&admission, &request.storage),
        key: ActivationKey {owner: control.execution.as_ref().unwrap().environments[&environment].owner.clone(), environment, component: worker.slug().clone()},
        generation: Generation::new(1).unwrap(),
        build: veoveo_types::BuildId::new(),
        deployment: worker.registration.deployment,
        idempotency: ApplyIdempotencyKey::parse("retained-fixture").unwrap(),
        inputs: BuildInputs {
            runtime_template: reference,
            definition_digest: Sha256Digest::from_bytes([5;32]),
            expected_source: None,
            recipe: serde_json::from_value(serde_json::json!({"worker":{"kind":"compiled","output":{"root":"source","path":"service"},"args":[],"cwd":"."}})).unwrap(),
        },
    };
    let capture = veoveo_hub_sdk::ResolvedComposition::new(
        environment,
        BTreeMap::from([(
            worker.slug().clone(),
            veoveo_hub_sdk::ResolvedComponent::Deployment {
                deployment: accepted.deployment,
            },
        )]),
    );
    hub.retain_composition(&capture).await.unwrap();
    let (_, route) = crate::state::HubState::new(db.a.clone())
        .create_task_route(crate::state::HubTaskRouteDraft {
            tenant: caller.authority.tenant.clone(),
            work_context: caller.authority.work_context.clone(),
            profile: veoveo_hub_sdk::HubProfileId::parse("fixture").unwrap(),
            server: worker.slug().clone(),
            deployment: accepted.deployment,
            execution: capture.context(),
            source_task_id: "retained-native-task".into(),
            authority_digest: Sha256Digest::from_bytes([6; 32]),
            ownership: crate::state::HubTaskOwnership::from_invocation(
                &caller.actor,
                &caller.authority,
            ),
            ttl_ms: Some(60_000),
            delegated_caller: None,
        })
        .await
        .unwrap();
    assert!(route.expires_at > Utc::now());
    superseded_worker_recovery_remains_in_retained_scan(&db, &accepted).await;
    control
        .execution
        .as_mut()
        .unwrap()
        .environments
        .get_mut(&environment)
        .unwrap()
        .components
        .remove(worker.slug());
    publish(&store, control.clone()).await;
    assert!(accepted.admit(&control).is_err()); // owned override was removed
    let retained_digest = Sha256Digest::from_bytes([7; 32]);
    let mut connection = db.a.pool().acquire().await.unwrap();
    RegistrationStore::admit_deployment_guarded_on(
        &mut connection,
        &request,
        &retained_digest,
        Some(WorkerAdmission::Retained(&accepted)),
    )
    .await
    .unwrap();
    drop(connection);
    let authority = RegistrationAuthority::Deployment(Box::new(
        store
            .deployment_admission(&retained_digest)
            .await
            .unwrap()
            .unwrap(),
    ));
    assert_eq!(
        register(&store, &authority, &worker, &worker_key).await.0,
        Pin::Held
    );
    control
        .execution
        .as_mut()
        .unwrap()
        .runtime_templates
        .clear();
    publish(&store, control.clone()).await;
    let mut connection = db.a.pool().acquire().await.unwrap();
    assert!(
        RegistrationStore::admit_deployment_guarded_on(
            &mut connection,
            &request,
            &Sha256Digest::from_bytes([8; 32]),
            Some(WorkerAdmission::Retained(&accepted))
        )
        .await
        .is_err()
    );
    drop(connection);
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

/// Exercise the actual current-worker recovery transition and a competing reserve,
/// then the production retained scan. A healthy serving publication is fixture
/// setup here; registration and its canonical Task were created above normally.
async fn superseded_worker_recovery_remains_in_retained_scan(
    db: &crate::test_store::TestDb,
    accepted: &AcceptedApply,
) {
    use crate::activation::{
        ActivationStore, ApplyIdempotencyKey, BuildFailure, FailureCode, MaterializedSource,
        RelativePath, VerifiedOutput,
    };
    let now = Utc::now();
    let tenant = deterministic_tenant_id(accepted.key.owner.tenant.as_str()).unwrap();
    let foreign_tenant = deterministic_tenant_id("foreign-tenant").unwrap();
    assert!(
        sqlx::query_file!(
            "src/registration/store/retained_task.sql",
            tenant as DatabaseTenantId,
            accepted.deployment.as_uuid(),
            now
        )
        .fetch_one(db.a.pool())
        .await
        .unwrap()
        .retained
    );
    assert!(
        !sqlx::query_file!(
            "src/registration/store/retained_task.sql",
            foreign_tenant as DatabaseTenantId,
            accepted.deployment.as_uuid(),
            now
        )
        .fetch_one(db.a.pool())
        .await
        .unwrap()
        .retained
    );
    sqlx::query_file!(
        "src/activation/insert.sql",
        accepted.build.as_uuid(),
        accepted.deployment.as_uuid(),
        accepted.key.owner.tenant.as_str(),
        accepted.key.owner.principal.as_str(),
        accepted.key.environment.as_uuid(),
        accepted.key.component.as_str(),
        accepted.generation.get(),
        accepted.idempotency.as_str(),
        Json(accepted) as _,
        now
    )
    .execute(db.a.pool())
    .await
    .unwrap();
    sqlx::query_file!(
        "src/activation/reserve.sql",
        accepted.key.owner.tenant.as_str(),
        accepted.key.owner.principal.as_str(),
        accepted.key.environment.as_uuid(),
        accepted.key.component.as_str(),
        accepted.generation.get(),
        accepted.build.as_uuid()
    )
    .execute(db.a.pool())
    .await
    .unwrap();
    let store = ActivationStore::new(db.a.clone());
    let source = MaterializedSource {
        identity: crate::activation::SnapshotIdentity {
            digest: Sha256Digest::from_bytes([9; 32]),
            size: 1,
        },
        directory: RelativePath::parse(format!("sources/{}", accepted.build)).unwrap(),
    };
    let output = VerifiedOutput {
        digest: Sha256Digest::from_bytes([10; 32]),
        executable: RelativePath::parse(format!("builds/{}/service", accepted.build)).unwrap(),
    };
    store.source_ready(accepted, source.clone()).await.unwrap();
    store.begin_build(accepted).await.unwrap();
    store.output_ready(accepted, output.clone()).await.unwrap();
    // Seed the earlier successful readiness/publication, then invoke the same
    // transition recovery uses when that serving worker becomes unavailable.
    let serving = BuildState::Serving {
        source: source.clone(),
        output: output.clone(),
    };
    sqlx::query_file!(
        "src/activation/transition.sql",
        accepted.build.as_uuid(),
        Json(&serving) as _,
        Utc::now()
    )
    .execute(db.a.pool())
    .await
    .unwrap();
    store.recover_worker(accepted).await.unwrap();
    assert!(matches!(
        store
            .status(&accepted.key.owner, accepted.build)
            .await
            .unwrap()
            .state,
        BuildState::Registering { .. }
    ));
    let newer = store
        .reserve(
            accepted.key.clone(),
            ApplyIdempotencyKey::parse("superseding-during-recovery").unwrap(),
            accepted.inputs.clone(),
        )
        .await
        .unwrap();
    assert!(newer.generation > accepted.generation);
    assert!(
        !store
            .status(&accepted.key.owner, accepted.build)
            .await
            .unwrap()
            .current
    );
    let registering = BuildState::Registering {
        source: source.clone(),
        output: output.clone(),
    };
    for (state, expected) in [
        (registering.clone(), true),
        (
            BuildState::Failed {
                failure: BuildFailure::Worker {
                    source: source.clone(),
                    output: output.clone(),
                    code: FailureCode::NativeUnavailable,
                },
            },
            true,
        ),
        (
            BuildState::Failed {
                failure: BuildFailure::Worker {
                    source: source.clone(),
                    output: output.clone(),
                    code: FailureCode::Registration,
                },
            },
            false,
        ),
        (
            BuildState::Failed {
                failure: BuildFailure::Worker {
                    source: source.clone(),
                    output: output.clone(),
                    code: FailureCode::Readiness,
                },
            },
            false,
        ),
        (
            BuildState::Failed {
                failure: BuildFailure::Build {
                    source: source.clone(),
                    code: FailureCode::NativeUnavailable,
                },
            },
            false,
        ),
        (BuildState::Withdrawn, false),
    ] {
        sqlx::query_file!(
            "src/activation/transition.sql",
            accepted.build.as_uuid(),
            Json(&state) as _,
            Utc::now()
        )
        .execute(db.a.pool())
        .await
        .unwrap();
        let rows = sqlx::query_file!(
            "src/bin/hub/activation/retained_list.sql",
            accepted.key.owner.tenant.as_str(),
            accepted.key.owner.principal.as_str(),
            accepted.key.environment.as_uuid(),
            uuid::Uuid::nil(),
            tenant as DatabaseTenantId,
            Utc::now(),
            crate::activation::ACTIVATION_PAGE_SIZE
        )
        .fetch_all(db.a.pool())
        .await
        .unwrap();
        assert_eq!(
            rows.iter().any(|row| row.accepted.0 == *accepted),
            expected,
            "retained scan state={state:?}"
        );
        if expected {
            assert_eq!(rows[0].state.0, state);
        }
    }
    sqlx::query_file!(
        "src/activation/transition.sql",
        accepted.build.as_uuid(),
        Json(&registering) as _,
        Utc::now()
    )
    .execute(db.a.pool())
    .await
    .unwrap();
    let after_expiry = Utc::now() + chrono::TimeDelta::minutes(2);
    assert!(
        sqlx::query_file!(
            "src/bin/hub/activation/retained_list.sql",
            accepted.key.owner.tenant.as_str(),
            accepted.key.owner.principal.as_str(),
            accepted.key.environment.as_uuid(),
            uuid::Uuid::nil(),
            tenant as DatabaseTenantId,
            after_expiry,
            crate::activation::ACTIVATION_PAGE_SIZE
        )
        .fetch_all(db.a.pool())
        .await
        .unwrap()
        .is_empty()
    );
}
