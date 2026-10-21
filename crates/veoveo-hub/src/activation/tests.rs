use super::*;
use crate::{
    control_plane::*,
    execution::*,
    store::{ControlPublication, HubStore},
};
use chrono::Utc;
use std::collections::BTreeMap;
use veoveo_hub_sdk::{EnvironmentId, ServerSlug, registration::StorageBinding};
use veoveo_types::{PrincipalId, Sha256Digest, TenantId};

fn key() -> ActivationKey {
    ActivationKey {
        owner: EnvironmentOwner {
            tenant: TenantId::parse("tenant").unwrap(),
            principal: PrincipalId::parse("owner").unwrap(),
        },
        environment: EnvironmentId::parse("019a5b30-0000-7000-8000-000000000001").unwrap(),
        component: ServerSlug::parse("observatory").unwrap(),
    }
}
fn inputs(byte: u8) -> BuildInputs {
    BuildInputs {
        runtime_template: RuntimeTemplateRef::parse("fixture-runtime-v1").unwrap(),
        definition_digest: Sha256Digest::from_bytes([byte;32]),
        expected_source: Some(SnapshotIdentity { digest: Sha256Digest::from_bytes([byte;32]), size: 1024 }),
        recipe: serde_json::from_value(serde_json::json!({"worker":{"kind":"compiled","output":{"root":"cache","path":"target/debug/observatory"},"args":[],"cwd":"."}})).unwrap(),
    }
}
fn control() -> HubControlPlane {
    let key = key();
    let mut control: HubControlPlane = serde_json::from_value(serde_json::json!({"identityProviders":[],"authorizationServers":[],"profiles":[],"tenants":[],"workContexts":[],"policies":[],"dataLabels":[],"oidcClients":[]})).unwrap();
    control.execution = Some(ExecutionConfiguration {
        default_environment: key.environment,
        environments: BTreeMap::from([(
            key.environment,
            EnvironmentRecord {
                owner: key.owner.clone(),
                display_name: "Fixture".into(),
                readers: Default::default(),
                base: None,
                components: BTreeMap::from([(
                    key.component.clone(),
                    veoveo_hub_sdk::ResolvedComponent::Unavailable {},
                )]),
            },
        )]),
        runtime_templates: BTreeMap::from([(
            inputs(1).runtime_template,
            RuntimeTemplateAdmission {
                workspace: "fixture-workspace".into(),
                template: "fixture-template-v1".into(),
                policy_file: "/etc/fixture-policy-v1.yaml".into(),
                providers: vec![],
                eligible: vec![RuntimeEligibility {
                    environment: key.environment,
                    component: key.component.clone(),
                    storage: StorageBinding::SharedLogicalSchema {
                        server: key.component,
                    },
                }],
            },
        )]),
    });
    control
}
fn revision(control_plane: HubControlPlane) -> HubControlPlaneRevision {
    HubControlPlaneRevision {
        revision_id: crate::new_hub_control_plane_revision_id().unwrap(),
        sha256: "0".repeat(64),
        source: HubControlPlaneRevisionSource::Contribution,
        applied_at: Utc::now(),
        applied_by: key().owner.principal,
        tenant: Some(key().owner.tenant),
        control_plane,
    }
}
async fn publish(store: &HubStore, revision: &HubControlPlaneRevision) {
    let active = store.active_control_revision().await.unwrap();
    store
        .publish_control_revision(
            ControlPublication {
                revision,
                replaces: active.as_ref().map(|a| &a.revision_id),
                objects: &[],
            },
            &crate::audit::AuditAppend::new(Default::default()),
        )
        .await
        .unwrap();
}

#[test]
fn invalid_recipe_and_state_are_rejected_at_the_wire_boundary() {
    for path in [
        "../outside",
        "/absolute",
        "a/../../b",
        "windows\\path",
        "a//b",
    ] {
        assert!(serde_json::from_value::<RelativePath>(serde_json::json!(path)).is_err());
    }
    for value in [0, -1] {
        assert!(serde_json::from_value::<Generation>(serde_json::json!(value)).is_err());
    }
    assert!(serde_json::from_value::<BuildState>(serde_json::json!({"phase":"serving"})).is_err());
    for argument in [
        "--deployment-id",
        "--port=99",
        "--registration-secret=caller-value",
    ] {
        assert!(serde_json::from_value::<ServiceRecipe>(serde_json::json!({"worker":{"kind":"compiled","output":{"root":"cache","path":"target/debug/service"},"args":[argument],"cwd":"."}})).is_err());
    }
    assert!(serde_json::from_value::<ServiceRecipe>(serde_json::json!({"build":[{"executable":"../outside","cwd":"."}],"worker":{"kind":"interpreted","interpreter":"/usr/bin/python3","entrypoint":"service.py","args":[],"cwd":"."}})).is_err());
}

#[tokio::test]
async fn competing_reservations_preserve_idempotency_and_stale_completion_cannot_publish() {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let db = crate::test_store::TestDb::new().await;
        let hub = HubStore::new(db.a.clone());
        let initial = revision(control());
        publish(&hub, &initial).await;
        let left = ActivationStore::new(db.a.clone());
        let right = ActivationStore::new(db.b.clone());
        let id = ApplyIdempotencyKey::parse("same-request").unwrap();
        let (a, b) = tokio::join!(
            left.reserve(key(), id.clone(), inputs(1)),
            right.reserve(key(), id.clone(), inputs(1))
        );
        let a = a.unwrap();
        assert_eq!(a, b.unwrap());
        assert!(
            left.reserve(key(), id, inputs(2))
                .await
                .unwrap_err()
                .is::<ActivationError>()
        );
        let source = MaterializedSource {
            identity: inputs(1).expected_source.unwrap(),
            directory: RelativePath::parse(format!("sources/{}", a.build)).unwrap(),
        };
        left.source_ready(&a, source.clone()).await.unwrap();
        left.begin_build(&a).await.unwrap();
        let output = VerifiedOutput {
            digest: Sha256Digest::from_bytes([9; 32]),
            executable: RelativePath::parse(format!("builds/{}/service", a.build)).unwrap(),
        };
        left.output_ready(&a, output).await.unwrap();
        let newer = right
            .reserve(
                key(),
                ApplyIdempotencyKey::parse("newer-request").unwrap(),
                inputs(2),
            )
            .await
            .unwrap();
        assert!(newer.generation > a.generation);
        assert!(
            left.source_ready(&a, source)
                .await
                .unwrap_err()
                .is::<ActivationError>()
        );
        let mut proposed = initial.control_plane.clone();
        proposed
            .execution
            .as_mut()
            .unwrap()
            .environments
            .get_mut(&key().environment)
            .unwrap()
            .components
            .insert(
                key().component,
                veoveo_hub_sdk::ResolvedComponent::Deployment {
                    deployment: a.deployment,
                },
            );
        let proposed = revision(proposed);
        let error = hub
            .publish_activation_revision(
                ControlPublication {
                    revision: &proposed,
                    replaces: Some(&initial.revision_id),
                    objects: &[],
                },
                &crate::audit::AuditAppend::new(Default::default()),
                &ActivationPublication {
                    accepted: a.clone(),
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(error, crate::store::HubStoreError::Activation(_)));
        assert_eq!(
            hub.active_control_revision()
                .await
                .unwrap()
                .unwrap()
                .revision_id,
            initial.revision_id
        );
        assert!(!left.status(&key().owner, a.build).await.unwrap().current);
        let current = right.status(&key().owner, newer.build).await.unwrap();
        assert!(current.current);
        assert!(current.serving.is_none());
        right.fail(&newer, FailureCode::Materialization).await.unwrap();
        assert!(matches!(
            right.status(&key().owner, newer.build).await.unwrap().state,
            BuildState::Failed { .. }
        ));
        right.retry(&newer).await.unwrap();
        assert!(matches!(right.status(&key().owner,newer.build).await.unwrap().state,BuildState::Preparing));
        let retained_source=MaterializedSource {identity:inputs(2).expected_source.unwrap(),directory:RelativePath::parse(format!("sources/{}",newer.build)).unwrap()};
        right.source_ready(&newer,retained_source.clone()).await.unwrap();
        right.fail(&newer,FailureCode::Build).await.unwrap();
        right.retry(&newer).await.unwrap();
        assert!(matches!(right.status(&key().owner,newer.build).await.unwrap().state,BuildState::SourceReady {source} if source==retained_source));
        right.begin_build(&newer).await.unwrap();
        let retained_output=VerifiedOutput {digest:Sha256Digest::from_bytes([8;32]),executable:RelativePath::parse(format!("builds/{}/service",newer.build)).unwrap()};
        right.output_ready(&newer,retained_output.clone()).await.unwrap();
        right.fail(&newer,FailureCode::Registration).await.unwrap();
        right.retry(&newer).await.unwrap();
        assert!(matches!(right.status(&key().owner,newer.build).await.unwrap().state,BuildState::Registering {output,..} if output==retained_output));
        let mut changed=retained_output; changed.digest=Sha256Digest::from_bytes([7;32]);
        assert!(right.output_ready(&newer,changed).await.unwrap_err().is::<ActivationError>());
        let change=EnvironmentPublication {owner:key().owner,environment:key().environment,display_name:"Still owned".into(),base:None,components:Default::default()};
        let withdrawn=revision(change.project(&initial.control_plane).unwrap());
        hub.publish_environment_revision(ControlPublication {revision:&withdrawn,replaces:Some(&initial.revision_id),objects:&[]},&crate::audit::AuditAppend::new(Default::default()),&change).await.unwrap();
        assert!(!right.status(&key().owner,newer.build).await.unwrap().current);
        assert!(right.retry(&newer).await.unwrap_err().is::<ActivationError>());
        let mut foreign = key();
        foreign.owner.principal = PrincipalId::parse("another-user").unwrap();
        assert!(
            right
                .reserve(
                    foreign,
                    ApplyIdempotencyKey::parse("foreign").unwrap(),
                    inputs(3)
                )
                .await
                .unwrap_err()
                .is::<ActivationError>()
        );
    })
    .await
    .expect("activation concurrency fixture exceeded one minute");
}

#[tokio::test]
async fn lost_singleton_connection_cannot_advance_or_fail_a_replacement_holders_build() {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let db = crate::test_store::TestDb::new().await;
        let hub = HubStore::new(db.a.clone());
        publish(&hub, &revision(control())).await;
        let store = ActivationStore::new(db.a.clone());
        let accepted = store.reserve(key(), ApplyIdempotencyKey::parse("owned-phase").unwrap(), inputs(1)).await.unwrap();
        let name = veoveo_store::SingletonName::parse("activation.fixture-owned-phase").unwrap();
        let mut former = veoveo_store::Singleton::acquire(&db.a, name.clone()).await.unwrap();
        let pid = sqlx::query_file!("src/activation/tests/role_backend.sql").fetch_one(former.connection()).await.unwrap().pid;
        assert!(sqlx::query_file!("src/activation/tests/terminate_role.sql", pid).fetch_one(db.b.pool()).await.unwrap().terminated);
        let mut current = veoveo_store::Singleton::acquire(&db.b, name).await.unwrap();
        let source = MaterializedSource {identity: inputs(1).expected_source.unwrap(), directory: RelativePath::parse(format!("sources/{}", accepted.build)).unwrap()};
        ActivationStore::source_ready_on(current.connection(), &accepted, source.clone()).await.unwrap();
        assert!(ActivationStore::begin_build_on(former.connection(), &accepted).await.is_err());
        assert!(ActivationStore::fail_on(former.connection(), &accepted, FailureCode::NativeUnavailable).await.is_err());
        assert!(matches!(store.status(&key().owner, accepted.build).await.unwrap().state, BuildState::SourceReady {source: retained} if retained == source));
        ActivationStore::begin_build_on(current.connection(), &accepted).await.unwrap();
        assert!(matches!(store.status(&key().owner, accepted.build).await.unwrap().state, BuildState::Building {..}));
        current.release().await.unwrap();
    }).await.expect("singleton ownership fixture exceeded one minute");
}

#[test]
fn retained_eligibility_survives_source_withdrawal_but_not_admission_or_owner_withdrawal() {
    let mut control = control();
    let key = key();
    let reference = inputs(1).runtime_template;
    assert!(key.admit(&control, &reference).is_ok());
    control
        .execution
        .as_mut()
        .unwrap()
        .environments
        .get_mut(&key.environment)
        .unwrap()
        .components
        .clear();
    assert!(matches!(
        key.admit(&control, &reference),
        Err(ActivationError::ComponentWithdrawn)
    ));
    assert!(key.admit_retained(&control, &reference).is_ok());
    control
        .execution
        .as_mut()
        .unwrap()
        .runtime_templates
        .get_mut(&reference)
        .unwrap()
        .eligible
        .clear();
    assert!(matches!(
        key.admit_retained(&control, &reference),
        Err(ActivationError::RuntimeNotAdmitted)
    ));
    let mut control = self::control();
    control
        .execution
        .as_mut()
        .unwrap()
        .environments
        .get_mut(&key.environment)
        .unwrap()
        .owner
        .principal = PrincipalId::parse("replacement-owner").unwrap();
    assert!(matches!(
        key.admit_retained(&control, &reference),
        Err(ActivationError::Forbidden)
    ));
    control.execution.as_mut().unwrap().environments.clear();
    assert!(matches!(
        key.admit_retained(&control, &reference),
        Err(ActivationError::UnknownEnvironment)
    ));
}
