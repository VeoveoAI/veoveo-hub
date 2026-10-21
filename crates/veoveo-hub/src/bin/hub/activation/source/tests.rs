use super::*;
use chrono::Utc;
use veoveo_hub::{
    control_plane::*,
    store::{ControlPublication, HubStore},
};
use veoveo_hub_sdk::ServerSlug;
use veoveo_types::{PrincipalId, TenantId};

struct NoAudit;
impl TransactionAppend for NoAudit {
    async fn append(&self, _: &mut PgConnection) -> Result<(), HubStoreError> {
        Ok(())
    }
}
fn authority() -> SourceAuthority {
    SourceAuthority {
        principal: PrincipalId::parse("fixture-owner").unwrap(),
        tenant: TenantId::parse("fixture-tenant").unwrap(),
        issuer: TokenIssuer::parse("https://hub.example/oauth").unwrap(),
        subject: TokenSubject::parse("fixture-owner").unwrap(),
        client: OAuthClientId::parse("fixture-client").unwrap(),
        profile: HubProfileId::parse("fixture-profile").unwrap(),
        work_context: WorkContextId::parse("fixture-context").unwrap(),
    }
}
fn definition() -> EnvironmentDefinition {
    serde_json::from_value(serde_json::json!({
        "id":"019a5b30-0000-7000-8000-000000000001","displayName":"Owned","base":null,
        "components":{
            "observatory":{"source":{"kind":"local_snapshot","digest":format!("sha256:{}","1".repeat(64)),"size":1024},"runtimeTemplate":"fixture-runtime-v1","recipe":{"worker":{"kind":"compiled","output":{"root":"cache","path":"target/debug/observatory"},"args":[],"cwd":"."}}},
            "fixture-two":{"source":{"kind":"local_snapshot","digest":format!("sha256:{}","2".repeat(64)),"size":1024},"runtimeTemplate":"fixture-runtime-v1","recipe":{"worker":{"kind":"compiled","output":{"root":"cache","path":"target/debug/fixture-two"},"args":[],"cwd":"."}}}
        }
    })).unwrap()
}
fn control() -> HubControlPlane {
    let definition = definition();
    let authority = authority();
    serde_json::from_value(serde_json::json!({
        "identityProviders":[],"authorizationServers":[],"profiles":[],"tenants":[],"workContexts":[],"policies":[],"dataLabels":[],"oidcClients":[],
        "execution":{"defaultEnvironment":definition.id,"environments":{definition.id.to_string():{"owner":{"tenant":authority.tenant,"principal":authority.principal},"displayName":"Empty","base":null,"components":{}}},
        "runtimeTemplates":{"fixture-runtime-v1":{"workspace":"fixture-workspace","template":"fixture-v1","policyFile":"/etc/fixture-v1.yaml","eligible":definition.components.keys().map(|component|serde_json::json!({"environment":definition.id,"component":component,"storage":{"kind":"shared_logical_schema","server":component}})).collect::<Vec<_>>()}}}
    })).unwrap()
}
fn revision(control_plane: HubControlPlane) -> HubControlPlaneRevision {
    HubControlPlaneRevision {
        revision_id: veoveo_hub::new_hub_control_plane_revision_id().unwrap(),
        sha256: "0".repeat(64),
        source: HubControlPlaneRevisionSource::Contribution,
        applied_at: Utc::now(),
        applied_by: authority().principal,
        tenant: Some(authority().tenant),
        control_plane,
    }
}
fn append(id: &str, definition: EnvironmentDefinition) -> DefinitionAppend {
    let authority = authority();
    DefinitionAppend {
        owner: owner(&authority.tenant, &authority.principal),
        environment: definition.id,
        idempotency: ApplyIdempotencyKey::parse(id).unwrap(),
        authority,
        definition,
        origin: crate::activation::definition_source::DefinitionOrigin::Local,
        accepted: Mutex::new(Vec::new()),
    }
}
async fn apply(store: &HubStore, request: &DefinitionAppend) -> Result<Vec<AcceptedApply>> {
    let active = store.active_control_revision().await?.unwrap();
    let mut connection = store.platform().pool().acquire().await?;
    if let Some(replayed) = request.replay_on(&mut connection).await? {
        return Ok(replayed);
    }
    drop(connection);
    let change = EnvironmentPublication {
        owner: request.owner.clone(),
        environment: request.environment,
        display_name: request.definition.display_name.clone(),
        base: request.definition.base.clone(),
        components: request.definition.components.keys().cloned().collect(),
    };
    let next = revision(change.project(&active.control_plane)?);
    store
        .publish_environment_revision(
            ControlPublication {
                revision: &next,
                replaces: Some(&active.revision_id),
                objects: &[],
            },
            request,
            &change,
        )
        .await?;
    Ok(request.accepted())
}

#[tokio::test]
async fn definition_roundtrip_changes_only_edited_components_and_replay_never_revives_old_builds() {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let db = veoveo_fixtures::store::TestDb::new().await;
        let store = HubStore::new(db.a.clone());
        store.migrate().await.unwrap();
        let initial = revision(control());
        store
            .publish_control_revision(
                ControlPublication {
                    revision: &initial,
                    replaces: None,
                    objects: &[],
                },
                &NoAudit,
            )
            .await
            .unwrap();
        let first = append("request-one", definition());
        let original = apply(&store, &first).await.unwrap();
        assert_eq!(original.len(), 2);
        let mut label = definition();
        label.display_name = "Renamed".into();
        assert_eq!(
            apply(&store, &append("label-only", label.clone()))
                .await
                .unwrap(),
            original
        );
        let changed_slug = ServerSlug::parse("observatory").unwrap();
        label.components.get_mut(&changed_slug).unwrap().source = SourceSelector::LocalSnapshot {
            snapshot: SnapshotIdentity {
                digest: Sha256Digest::from_bytes([3; 32]),
                size: 2048,
            },
        };
        let updated = apply(&store, &append("changed-one", label.clone()))
            .await
            .unwrap();
        for old in &original {
            let new = updated
                .iter()
                .find(|n| n.key.component == old.key.component)
                .unwrap();
            if old.key.component == changed_slug {
                assert_ne!(new.build, old.build);
                assert!(new.generation > old.generation);
            } else {
                assert_eq!(new, old);
            }
        }
        let active_before_replay = store
            .active_control_revision()
            .await
            .unwrap()
            .unwrap()
            .revision_id;
        assert_eq!(apply(&store, &first).await.unwrap(), original);
        assert_eq!(
            store
                .active_control_revision()
                .await
                .unwrap()
                .unwrap()
                .revision_id,
            active_before_replay
        );
        assert!(
            apply(&store, &append("request-one", label.clone()))
                .await
                .unwrap_err()
                .is::<ActivationError>()
        );
        let mut connection = store.platform().pool().acquire().await.unwrap();
        assert_eq!(
            definitions(&mut connection, &first.owner, first.environment)
                .await
                .unwrap(),
            label.components
        );
        drop(connection);
        let removed = ServerSlug::parse("fixture-two").unwrap();
        let previous = updated.iter().find(|n| n.key.component == removed).unwrap();
        let mut withdrawal = label.clone();
        withdrawal.components.remove(&removed);
        apply(&store, &append("withdraw-one", withdrawal))
            .await
            .unwrap();
        let restored = apply(&store, &append("restore-one", label)).await.unwrap();
        let replacement = restored
            .iter()
            .find(|n| n.key.component == removed)
            .unwrap();
        assert_ne!(replacement.build, previous.build);
        assert_ne!(replacement.deployment, previous.deployment);
        assert!(replacement.generation > previous.generation);
    })
    .await
    .expect("source definition fixture exceeded one minute");
}

#[tokio::test]
async fn late_definition_follow_response_cannot_replace_explicit_local_apply() {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        use crate::activation::definition_source::{self, DefinitionMode, DefinitionSourceState};
        let db = veoveo_fixtures::store::TestDb::new().await;
        let store = HubStore::new(db.a.clone());
        store.migrate().await.unwrap();
        let initial = revision(control());
        store
            .publish_control_revision(
                ControlPublication {
                    revision: &initial,
                    replaces: None,
                    objects: &[],
                },
                &NoAudit,
            )
            .await
            .unwrap();
        let original = append("initial-local", definition());
        apply(&store, &original).await.unwrap();
        let mut connection = store.platform().pool().acquire().await.unwrap();
        let first = definition_source::read_on(&mut connection, original.environment)
            .await
            .unwrap()
            .unwrap();
        // Fixture represents a prior authorized Follow admission. The late native
        // reply must be rejected before issuing any new native grant/publication.
        let followed = DefinitionSourceState {
            generation: Generation::new(first.generation.get() + 1).unwrap(),
            mode: DefinitionMode::Lore {
                source: LoreDefinitionSource {
                    deployment: crate::native_source_ids::DeploymentId::parse("fixture-native")
                        .unwrap(),
                    repository: crate::native_source_ids::RepositoryId::parse("1".repeat(32))
                        .unwrap(),
                    branch: crate::native_source_ids::BranchId::parse("2".repeat(32)).unwrap(),
                    path: RelativePath::parse("environments/work.json").unwrap(),
                },
                observed: None,
            },
            ..first
        };
        definition_source::persist_on(&mut connection, &followed)
            .await
            .unwrap();
        drop(connection);
        let mut local = definition();
        local.display_name = "Explicit local selection".into();
        let current = apply(&store, &append("new-local", local.clone()))
            .await
            .unwrap();
        let late = DefinitionAppend::from_follow(
            followed.clone(),
            NativeRevision::parse("a".repeat(64)).unwrap(),
            definition(),
        )
        .unwrap();
        assert!(apply(&store, &late).await.is_err());
        let active = store.active_control_revision().await.unwrap().unwrap();
        assert_eq!(
            active.control_plane.execution.unwrap().environments[&original.environment]
                .display_name,
            local.display_name
        );
        let mut connection = store.platform().pool().acquire().await.unwrap();
        let saved = definition_source::read_on(&mut connection, original.environment)
            .await
            .unwrap()
            .unwrap();
        assert!(saved.generation > followed.generation);
        assert_eq!(saved.mode, DefinitionMode::Local);
        assert_eq!(
            definitions(&mut connection, &original.owner, original.environment)
                .await
                .unwrap(),
            local.components
        );
        let later = DefinitionAppend::from_follow(
            DefinitionSourceState {
                generation: Generation::new(followed.generation.get() + 2).unwrap(),
                ..followed
            },
            NativeRevision::parse("a".repeat(64)).unwrap(),
            definition(),
        )
        .unwrap();
        assert_ne!(late.idempotency, later.idempotency); // A→B→A does not replay historical A
        assert_eq!(current.len(), 2);
    })
    .await
    .expect("definition source race fixture exceeded one minute");
}

#[tokio::test]
async fn scoped_reconciliation_pages_keep_every_owner_and_withdraw_the_last_page() {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let db = veoveo_fixtures::store::TestDb::new().await;
        let store = HubStore::new(db.a.clone());
        store.migrate().await.unwrap();
        let mut first = definition();
        let mut component = first.components.values().next().unwrap().clone();
        component.source = SourceSelector::Lore {
            deployment: crate::native_source_ids::DeploymentId::parse("fixture-native").unwrap(),
            repository: crate::native_source_ids::RepositoryId::parse("1".repeat(32)).unwrap(),
            selection: LoreSelection::Follow {
                branch: crate::native_source_ids::BranchId::parse("2".repeat(32)).unwrap(),
            },
        };
        let count = usize::try_from(ACTIVATION_PAGE_SIZE).unwrap() + 1;
        first.components = (0..count)
            .map(|i| {
                (
                    ServerSlug::parse(format!("fixture-{i:03}")).unwrap(),
                    component.clone(),
                )
            })
            .collect();
        let mut second = first.clone();
        second.id = veoveo_hub_sdk::EnvironmentId::new();
        let mut second_authority = authority();
        second_authority.tenant = TenantId::parse("other-tenant").unwrap();
        second_authority.principal = PrincipalId::parse("other-owner").unwrap();
        let mut initial = control();
        let execution = initial.execution.as_mut().unwrap();
        let mut other_record = execution.environments[&first.id].clone();
        other_record.owner = owner(&second_authority.tenant, &second_authority.principal);
        execution.environments.insert(second.id, other_record);
        let admission = execution.runtime_templates.values_mut().next().unwrap();
        admission.eligible = [first.id, second.id]
            .into_iter()
            .flat_map(|environment| {
                first
                    .components
                    .keys()
                    .map(move |component| RuntimeEligibility {
                        environment,
                        component: component.clone(),
                        storage:
                            veoveo_hub_sdk::registration::StorageBinding::SharedLogicalSchema {
                                server: component.clone(),
                            },
                    })
            })
            .collect();
        let initial = revision(initial);
        store
            .publish_control_revision(
                ControlPublication {
                    revision: &initial,
                    replaces: None,
                    objects: &[],
                },
                &NoAudit,
            )
            .await
            .unwrap();
        let first_request = append("first-owner-pages", first.clone());
        let mut second_request = append("second-owner-pages", second.clone());
        second_request.owner = owner(&second_authority.tenant, &second_authority.principal);
        second_request.authority = second_authority;
        let first_builds = apply(&store, &first_request).await.unwrap();
        let second_builds = apply(&store, &second_request).await.unwrap();
        for request in [&first_request, &second_request] {
            let mut connection = db.a.pool().acquire().await.unwrap();
            assert_eq!(
                definitions(&mut connection, &request.owner, request.environment)
                    .await
                    .unwrap(),
                request.definition.components,
                "full definition must include the component beyond the first page",
            );
            let mut after = String::new();
            let mut followed = Vec::new();
            let mut recovered = Vec::new();
            let mut pages = 0;
            loop {
                let page = sqlx::query_file!(
                    "src/bin/hub/activation/follow_list.sql",
                    request.owner.tenant.as_str(),
                    request.owner.principal.as_str(),
                    request.environment.as_uuid(),
                    &after,
                    ACTIVATION_PAGE_SIZE
                )
                .fetch_all(&mut *connection)
                .await
                .unwrap();
                let Some(last) = page.last() else {
                    break;
                };
                after.clone_from(&last.component);
                pages += 1;
                for row in page {
                    assert_eq!(row.accepted.0.key.owner, request.owner);
                    assert_eq!(row.accepted.0.key.environment, request.environment);
                    followed.push(row.accepted.0.build);
                }
            }
            after.clear();
            loop {
                let page = sqlx::query_file!(
                    "src/bin/hub/activation/recover_list.sql",
                    request.owner.tenant.as_str(),
                    request.owner.principal.as_str(),
                    request.environment.as_uuid(),
                    &after,
                    ACTIVATION_PAGE_SIZE
                )
                .fetch_all(&mut *connection)
                .await
                .unwrap();
                let Some(last) = page.last() else {
                    break;
                };
                after.clone_from(&last.component);
                recovered.extend(page.into_iter().map(|row| row.accepted.0.build));
            }
            assert_eq!(pages, 2);
            assert_eq!(followed.len(), count);
            assert_eq!(recovered, followed);
            assert_eq!(
                followed
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                count
            );
        }
        let mut removed = first;
        removed.components.clear();
        apply(&store, &append("withdraw-all-first-owner-pages", removed))
            .await
            .unwrap();
        let activation = ActivationStore::new(db.a.clone());
        for build in first_builds {
            assert!(
                !activation
                    .status(&first_request.owner, build.build)
                    .await
                    .unwrap()
                    .current
            );
        }
        for build in second_builds {
            assert!(
                activation
                    .status(&second_request.owner, build.build)
                    .await
                    .unwrap()
                    .current
            );
        }
    })
    .await
    .expect("scoped reconciliation page fixture exceeded one minute");
}
