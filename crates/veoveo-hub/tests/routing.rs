//! Routing without named environments, against the real Hub binary and SDK fixture.
//! Registering is serving: a server needs no enrollment. When several deployments of
//! one slug are registered, the newest serves new requests. A retained Task and the
//! calls it makes keep the routing their request captured, also across a Hub restart.
//! The retired `veoveo-environment` header is ignored. Revisions register under their
//! synthetic namespace signer; no native runtime or source transfer is simulated.
#[path = "support/hub.rs"]
mod hub;
#[path = "support/ledger_revisions.rs"]
mod revisions;
#[path = "support/servers.rs"]
mod servers;

use hub::Hub;
use revisions::{FirstRevision, InspectionRelay, SecondRevision};
use rmcp::{
    RoleClient,
    model::{
        CallToolRequestParams, CallToolResponse, ClientConfig, ClientRequest, GetTaskParams,
        ListToolsRequest, ListToolsResult, ServerResult, TaskPayload,
    },
    service::{RunningService, ServiceError},
};
use serde_json::json;
use servers::{Served, hosted, mcp_client, mcp_client_with, relay_call, serve, serve_revision};
use std::{collections::BTreeSet, time::Duration};
use veoveo_fixtures::store::TestDb;
use veoveo_hub_sdk::{
    ExecutionContext, HubInternalIdentity, HubInternalTokenVerifier,
    hosting::{DomainServer, Hosted, ServerCalls},
    registration::client::Registration,
};
use veoveo_hub_sdk_fixture::ledger::{self, LedgerDomain, LedgerTasks};
use veoveo_store::{Instance, PlatformStore, StoreConfig};
use veoveo_types::{PermissionName, ServerSlug};

type Client = RunningService<RoleClient, ClientConfig>;

async fn process(registration: &Registration, slug: &str) -> (PlatformStore, Instance) {
    let grant = registration.database().unwrap();
    let store = PlatformStore::connect(
        StoreConfig::builder(grant.url.as_str(), grant.password.clone())
            .username(grant.username.as_str())
            .max_connections(2)
            .build()
            .unwrap(),
    )
    .await
    .unwrap();
    let instance = Instance::start(&store, &ServerSlug::parse(slug).unwrap())
        .await
        .unwrap();
    (store, instance)
}

/// Verify the last bearer against the actual recipient's registration response.
/// The outer recorder runs before hosted admission, so decoding alone is insufficient.
fn verify_identity(server: &Served, bearer: &str) -> HubInternalIdentity {
    let trust = server.registration.access_tokens();
    assert_eq!(trust.deployment, server.metadata.registration.deployment);
    assert_eq!(
        trust.resource,
        veoveo_hub_sdk::registration::ResourceIndicator::from(
            &server.metadata.registration.endpoint
        )
    );
    let identity =
        HubInternalTokenVerifier::from_access_tokens(trust, server.metadata.slug().clone())
            .unwrap()
            .verify(bearer)
            .expect("forwarded assertion verifies for the registration that received it");
    assert_eq!(&identity.server, server.metadata.slug());
    assert_eq!(identity.deployment, trust.deployment);
    assert_eq!(identity.audience, trust.resource);
    assert_eq!(identity.profile.as_str(), "local");
    assert_eq!(identity.client.as_str(), "acceptance-member");
    let context = identity
        .request_context
        .as_ref()
        .expect("typed request context");
    context
        .validate_for(&identity.actor, &identity.authority)
        .unwrap();
    assert_eq!(context.access_token.oauth_client_id, identity.client);
    identity
}

fn received_identity(server: &Served) -> HubInternalIdentity {
    verify_identity(server, &server.bearers.last())
}

/// A single snapshot includes every HTTP frame, but only unique verified Hub
/// request IDs count as distinct originating calls. GET/DELETE session cleanup
/// can arrive after a successful Task reply with that earlier call's context.
fn received_requests(server: &Served) -> (usize, BTreeSet<veoveo_audit_contract::AuditRequestId>) {
    let bearers = server.bearers.snapshot();
    let origins = bearers
        .iter()
        .map(|bearer| {
            verify_identity(server, bearer)
                .request_context
                .unwrap()
                .audit
                .id
        })
        .collect();
    (bearers.len(), origins)
}

/// The verified routing context the selected recipient last received.
fn capture(server: &Served) -> ExecutionContext {
    received_identity(server).request_context.unwrap().execution
}

fn permissions(names: &[&str]) -> BTreeSet<PermissionName> {
    names
        .iter()
        .map(|name| PermissionName::parse(*name).unwrap())
        .collect()
}

/// Authoritative catalog reads reach the Hub, rather than the native client's private
/// five-second response cache.
async fn wire_tools(client: &Client) -> ListToolsResult {
    match client
        .send_request(ClientRequest::ListToolsRequest(ListToolsRequest {
            method: Default::default(),
            params: None,
            extensions: Default::default(),
        }))
        .await
        .unwrap()
    {
        ServerResult::ListToolsResult(result) => result,
        other => panic!("tools/list returned an unexpected result: {other:?}"),
    }
}

async fn descriptor(client: &Client) -> String {
    let tools = wire_tools(client).await.tools;
    let descriptors = tools
        .iter()
        .filter(|tool| tool.name == "ledger__inspect")
        .collect::<Vec<_>>();
    assert_eq!(descriptors.len(), 1, "one deployment advertises the tool");
    descriptors[0].description.as_deref().unwrap().to_owned()
}

async fn inspect(client: &Client) {
    let result = client
        .call_tool(CallToolRequestParams::new("ledger__inspect").with_arguments(Default::default()))
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true), "{result:?}");
    assert_eq!(
        result.structured_content.unwrap()["client"],
        "acceptance-member"
    );
}

async fn task(client: &Client) -> String {
    match client
        .call_tool_once(
            CallToolRequestParams::new("ledger__tally")
                .with_arguments(json!({"entries":7}).as_object().unwrap().clone()),
        )
        .await
        .unwrap()
    {
        CallToolResponse::Task(created) => created.task.task_id,
        other => panic!("fixture did not create a Task: {other:?}"),
    }
}

async fn payload(client: &Client, task: &str) -> TaskPayload {
    client
        .get_task(GetTaskParams::new(task))
        .await
        .unwrap()
        .task
        .payload
}

#[tokio::test]
async fn the_newest_registered_deployment_serves_and_captures_keep_their_routing() {
    tokio::time::timeout(Duration::from_secs(120), scenario())
        .await
        .expect("routing scenario exceeded two minutes");
}

async fn scenario() {
    let started = std::time::Instant::now();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    let mut hub = Hub::start(directory.path(), &db, Duration::from_secs(5)).await;
    let http = reqwest::Client::new();
    let mut processes = Vec::new();
    let base_tasks = LedgerTasks::new(Duration::from_secs(8));
    let base = serve(
        &hub,
        &http,
        "ledger",
        ledger::authorization().unwrap(),
        async |registration, _, _| {
            processes.push(process(registration, "ledger").await);
            let tasks = base_tasks.clone();
            hosted::<LedgerDomain>(registration, &http)
                .handler(move || ledger::hosted(tasks.clone()))
                .build()
        },
    )
    .await;
    let relay = serve(
        &hub,
        &http,
        "relay",
        InspectionRelay::routes().authorization(),
        async |registration, metadata, key| {
            processes.push(process(registration, "relay").await);
            let calls = ServerCalls::discover(registration, metadata, key, http.clone())
                .await
                .unwrap();
            hosted::<InspectionRelay>(registration, &http)
                .calls(calls)
                .handler(|| Hosted::new(InspectionRelay))
                .build()
        },
    )
    .await;
    let token = hub.access_token(&http, hub.caller("member")).await;
    let client = mcp_client(&hub, token.clone()).await;

    // A registered server serves at once: nothing enrolls it.
    let base_descriptor = descriptor(&client).await;
    assert_eq!(base_descriptor, "Inspect caller identity");
    inspect(&client).await;
    let base_capture = capture(&base);
    let retained = task(&client).await;
    assert!(matches!(
        payload(&client, &retained).await,
        TaskPayload::Working
    ));
    assert_eq!(capture(&base), base_capture);

    // A newer deployment of the same slug serves new requests; the older one is set
    // aside, not unavailable.
    let first = serve_revision(
        &hub,
        &http,
        "ledger",
        FirstRevision::routes().authorization(),
        &base.key,
        async |registration, _, _| {
            processes.push(process(registration, "ledger").await);
            hosted::<FirstRevision>(registration, &http)
                .handler(|| Hosted::new(FirstRevision).with_tasks(LedgerTasks::default()))
                .build()
        },
    )
    .await;
    let first_descriptor = "Inspect identity from source revision A";
    tokio::time::timeout(Duration::from_secs(10), async {
        while descriptor(&client).await != first_descriptor {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the newest registered deployment serves new requests");
    inspect(&client).await;
    let first_capture = capture(&first);
    assert_ne!(first_capture.composition, base_capture.composition);

    // The header that once selected a named environment changes nothing.
    let selecting = mcp_client_with(
        &hub,
        token.clone(),
        &[("veoveo-environment", "019a05cb-6c00-7000-8712-3456789abcdf")],
    )
    .await;
    assert_eq!(descriptor(&selecting).await, first_descriptor);
    selecting.cancel().await.unwrap();

    // Delegation through the relay routes with the request's capture.
    let delegated = relay_call(&client, "relay_inspect", json!({})).await;
    assert_eq!(delegated.unwrap()["acting_server"], "relay");
    assert_eq!(capture(&first).composition, capture(&relay).composition);

    // The retained Task keeps the deployment and routing that created it.
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match payload(&client, &retained).await {
                TaskPayload::Completed { result } => break result,
                TaskPayload::Failed { error } => panic!("retained Task failed: {error:?}"),
                _ => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(result["structuredContent"]["entries"], 7);
    assert_eq!(capture(&base), base_capture);

    // A third deployment with a new typed argument takes over in turn.
    let second = serve_revision(
        &hub,
        &http,
        "ledger",
        SecondRevision::routes().authorization(),
        &base.key,
        async |registration, _, _| {
            processes.push(process(registration, "ledger").await);
            hosted::<SecondRevision>(registration, &http)
                .handler(|| Hosted::new(SecondRevision).with_tasks(LedgerTasks::default()))
                .build()
        },
    )
    .await;
    let second_descriptor = "Inspect identity from source revision B";
    tokio::time::timeout(Duration::from_secs(10), async {
        while descriptor(&client).await != second_descriptor {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the newest registered deployment serves new requests");
    // Each change of the set-aside deployments is logged once: the base, then also
    // the first revision.
    let warnings = || {
        hub.log()
            .matches("several deployments of a server are registered")
            .count()
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        while warnings() < 2 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the Hub logs the deployments it sets aside");
    assert_eq!(
        warnings(),
        2,
        "the Hub logs a set-aside deployment once per change"
    );
    let probed = client
        .call_tool(
            CallToolRequestParams::new("ledger__inspect").with_arguments(
                json!({"probe": "revision-b-only"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_ne!(probed.is_error, Some(true), "{probed:?}");
    assert_eq!(
        probed.structured_content.unwrap()["probe"],
        "revision-b-only"
    );

    // The Task's route is exact after a Hub restart too, and the Hub resumes routing
    // to the newest deployment.
    client.cancel().await.unwrap();
    hub.restart().await;
    let after = mcp_client(&hub, hub.access_token(&http, hub.caller("member")).await).await;
    assert!(matches!(
        payload(&after, &retained).await,
        TaskPayload::Completed { .. }
    ));
    assert_eq!(capture(&base), base_capture);
    assert_eq!(descriptor(&after).await, second_descriptor);

    // A retained Task pins deployment/composition, not the grants at creation.
    // Keep this caller/token fixed while publishing current installation policy.
    let before_policy = received_identity(&base);
    assert_eq!(
        before_policy.permissions,
        permissions(&["ledger.entries.read", "ledger.entries.write"])
    );
    let mut plane = hub.control_plane().await;
    let member = plane["authorization"]["roles"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|role| role["name"] == "member")
        .unwrap();
    member["access"] = json!(["read"]);
    hub.publish_control_plane(&plane).await;
    // This single-process admin handler replaces the local catalog synchronously
    // before its successful response (bin/hub/admin/control_plane.rs).
    assert!(matches!(
        payload(&after, &retained).await,
        TaskPayload::Completed { .. }
    ));
    let current = received_identity(&base);
    assert_eq!(current.permissions, permissions(&["ledger.entries.read"]));
    assert_eq!(current.deployment, before_policy.deployment);
    assert_eq!(current.audience, before_policy.audience);
    // Access-token verification stamps authentication time per HTTP request, even
    // when the bearer is unchanged; all other current actor fields stay equal.
    assert!(
        current.actor.authenticated_at.unwrap() >= before_policy.actor.authenticated_at.unwrap()
    );
    let mut expected_actor = before_policy.actor.clone();
    expected_actor.authenticated_at = current.actor.authenticated_at;
    assert_eq!(current.actor, expected_actor);
    assert_eq!(current.authority, before_policy.authority);
    let previous_context = before_policy.request_context.as_ref().unwrap();
    let current_context = current.request_context.as_ref().unwrap();
    assert_eq!(current_context.execution, base_capture);
    assert_eq!(current_context.access_token, previous_context.access_token);
    assert!(
        current_context.principal.authenticated_at.unwrap()
            >= previous_context.principal.authenticated_at.unwrap()
    );
    let mut expected_principal = previous_context.principal.clone();
    expected_principal.authenticated_at = current_context.principal.authenticated_at;
    assert_eq!(current_context.principal, expected_principal);
    eprintln!(
        "retained Task current grant: deployment={} composition={} old_permissions={:?} current_permissions={:?}",
        current.deployment,
        current_context.execution.composition.digest(),
        before_policy.permissions,
        current.permissions
    );

    // With the member grant removed, the same canonical Task and caller are denied
    // without a new forwarded request context; neither historical nor newest
    // routing is a fallback around current policy. Old session cleanup may continue.
    plane["authorization"]["grants"]
        .as_array_mut()
        .unwrap()
        .retain(|grant| grant["role"] != "member");
    hub.publish_control_plane(&plane).await;
    let before = [&base, &first, &second, &relay].map(received_requests);
    let refused = after.get_task(GetTaskParams::new(&retained)).await;
    match &refused {
        Ok(_) => eprintln!("retained Task after revocation: result=success"),
        Err(ServiceError::McpError(error)) => eprintln!(
            "retained Task after revocation: result=McpError code={:?} roles_denial={}",
            error.code,
            error.message.contains("Your roles don't grant")
        ),
        Err(error) => eprintln!("retained Task after revocation: result=other_error {error}"),
    }
    let observed = [&base, &first, &second, &relay].map(received_requests);
    for (name, (old, new)) in ["base", "first", "second", "relay"]
        .into_iter()
        .zip(before.iter().zip(&observed))
    {
        eprintln!(
            "retained Task refusal observation: recipient={name} frames_before={} frames_after={} added_request_ids={:?}",
            old.0,
            new.0,
            new.1.difference(&old.1).collect::<Vec<_>>()
        );
    }
    for (old, new) in before.iter().zip(&observed) {
        assert_eq!(
            new.1, old.1,
            "current policy refusal must create no new forwarded request context"
        );
    }
    match refused {
        Err(ServiceError::McpError(error)) => {
            assert!(
                error.message.contains("Your roles don't grant"),
                "{error:?}"
            );
        }
        other => panic!("expected a current-policy refusal, got {other:?}"),
    }
    eprintln!(
        "routing scenario: ledger_base={} ledger_first={} ledger_second={} retained_task={} base_composition={} first_composition={} elapsed_ms={}",
        base.metadata.registration.deployment,
        first.metadata.registration.deployment,
        second.metadata.registration.deployment,
        retained,
        base_capture.composition.digest(),
        first_capture.composition.digest(),
        started.elapsed().as_millis()
    );
    after.cancel().await.unwrap();
    for (store, instance) in processes {
        instance.shutdown().await.unwrap();
        store.close().await;
    }
    for served in [base, first, second, relay] {
        served.client.deregister(served.registration).await.unwrap();
    }
    drop(hub);
}
