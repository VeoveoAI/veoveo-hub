//! Tasks that an ordinary tool's call starts, through the Hub binary: a fixture server
//! this test declares, Spawner, starts a durable Task from a plain `tools/call` and names
//! it in its result's `_meta["io.modelcontextprotocol/related-task"]`. The Hub records it
//! as it records a Task tool's Task: the caller gets the canonical identifier, follows the
//! Task, and the Task exchanges for its caller. A Task another server's call started
//! keeps no delegation, and a result that names a related task otherwise than as one
//! Task identifier is the server's protocol error.

#[path = "support/hub.rs"]
mod hub;
#[path = "support/servers.rs"]
mod servers;

use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use axum::{body::Body, extract::Request, middleware::Next, response::Response};
use hub::Hub;
use rmcp::{
    ErrorData, RoleServer,
    model::{
        CallToolRequestParams, CallToolResult, CreateTaskResult, GetTaskParams, GetTaskResult,
        Implementation, ServerConfig, TaskPayload, UpdateTaskParams,
    },
    service::RequestContext,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use servers::{decoded_claims, exchange, hosted, mcp_client, serve, serve_wrapped};
use veoveo_fixtures::store::TestDb;
use veoveo_hub_sdk::{
    HubInternalIdentity, RELATED_TASK_META_KEY,
    calls::{ProviderContract, TASK_TOKEN_TYPE},
    docs::ServerDocs,
    durable_tasks::{
        CallTask, DurableTaskService, DurableTaskSubscription, DurableTasks, cancel_durable_task,
        get_durable_task, start_call_task, subscribe_durable_tasks, task_owner,
        update_durable_task,
    },
    hosting::{
        ClientContext, DomainServer, Hosted, RoutePolicy, Routes, ServerCalls, ToolError,
        hub_identity,
    },
    hub::HubToolName,
    registration::{Access, PermissionDefinition},
    server_contract::{McpServerContract, McpServerSetup},
};
use veoveo_store::{Instance, PlatformStore, StoreConfig};
use veoveo_task::{RecoveryClass, TaskRuntime};
use veoveo_types::{PermissionName, ResourceScheme, ServerSlug, TaskTypeName};

/// The time to live Spawner gives its Tasks, which their routes keep.
const TASK_TTL_MS: u64 = 123_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
pub enum NoScope {}

/// A server this test declares, sharing one way to state its contract.
macro_rules! fixture {
    ($slug:literal, $provider:ident, $contract:ident, $domain:ty, $routes:ident) => {
        pub struct $provider;
        impl ProviderContract for $provider {
            fn slug() -> ServerSlug {
                ServerSlug::parse($slug).unwrap()
            }
        }
        pub struct $contract;
        impl McpServerContract for $contract {
            type Server = $domain;
            type Provider = $provider;
            type Scope = NoScope;
            fn scheme() -> ResourceScheme {
                ResourceScheme::parse($slug).unwrap()
            }
            fn scopes() -> &'static [NoScope] {
                &[]
            }
            fn documents() -> &'static ServerDocs {
                static DOCS: LazyLock<ServerDocs> = LazyLock::new(|| {
                    ServerDocs::new($provider::slug())
                        .with_doc("agents", "Agents", "# Fixture")
                        .with_doc("design", "Design", "# Fixture for Hub tests only")
                });
                &DOCS
            }
            fn server_config() -> ServerConfig {
                let mut config = ServerConfig::default();
                config.server_info = Implementation::new($slug, "0.1.0");
                config
            }
        }
        impl DomainServer for $domain {
            type Contract = $contract;
            fn setup() -> &'static McpServerSetup<$contract> {
                static SETUP: LazyLock<McpServerSetup<$contract>> =
                    LazyLock::new(|| McpServerSetup::new().unwrap());
                &SETUP
            }
            fn routes() -> &'static Routes<Self> {
                &$routes
            }
        }
    };
}

/// Spawner's one permission and Forwarder's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
pub enum FixturePermission {
    #[vocabulary(rename = "spawner.tasks.start")]
    TasksStart,
    #[vocabulary(rename = "forwarder.calls.make")]
    CallsMake,
}
impl PermissionDefinition for FixturePermission {
    fn resource(self) -> &'static str {
        match self {
            Self::TasksStart => "spawner.task",
            Self::CallsMake => "forwarder.call",
        }
    }
    fn access(self) -> Access {
        Access::Write
    }
}

fn permission(permission: FixturePermission) -> PermissionName {
    PermissionName::parse(permission.as_str()).unwrap()
}

/// How Spawner names the Task its call started: as one Task, as the Hub expects, or, by
/// rewriting its own answer, malformed or as two.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Naming {
    One,
    Malformed,
    Two,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[veoveo_hub_sdk::tool(provider=SpawnerProvider,output=Started,name="spawner_start")]
pub struct SpawnerStart {
    pub naming: Naming,
}

/// The Task Spawner started, by the identifier Spawner issued.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, veoveo_hub_sdk::SingleOutput)]
pub struct Started {
    pub source: String,
    pub naming: Naming,
}

#[derive(Clone)]
pub struct SpawnerDomain {
    runtime: TaskRuntime,
}

static SPAWNER_ROUTES: LazyLock<Routes<SpawnerDomain>> = LazyLock::new(|| {
    Routes::builder(RoutePolicy::Authenticated)
        .tool::<SpawnerStart, _, _>(
            "Start a Task in the background",
            RoutePolicy::permission(FixturePermission::TasksStart).unwrap(),
            |domain: SpawnerDomain, request, context| async move {
                domain.start(request, context).await
            },
        )
        // A server that calls others may call them from its Tasks for their caller.
        .uses(permission(FixturePermission::CallsMake))
        .starts_tasks()
        .build()
});
fixture!(
    "spawner",
    SpawnerProvider,
    SpawnerContract,
    SpawnerDomain,
    SPAWNER_ROUTES
);

impl SpawnerDomain {
    async fn start(
        self,
        request: SpawnerStart,
        context: RequestContext<RoleServer>,
    ) -> Result<Started, ToolError> {
        let started = start_call_task(
            &self.runtime,
            &context,
            CallTask {
                task_type: TaskTypeName::from_static("spawner.wait"),
                request: json!({}),
                recovery_class: RecoveryClass::Resume,
                ttl_ms: Some(TASK_TTL_MS),
                poll_interval_ms: Some(500),
            },
        )
        .await?;
        Ok(Started {
            source: started.task_id.to_string(),
            naming: request.naming,
        })
    }
}

/// Spawner's Tasks for their callers. No worker runs them: they stay working.
#[derive(Clone)]
pub struct SpawnerTasks {
    runtime: TaskRuntime,
}

impl DurableTaskService for SpawnerTasks {
    type Caller = HubInternalIdentity;

    fn authenticate(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<Self::Caller, ErrorData> {
        hub_identity(context)
    }

    async fn start_tool_task(
        &self,
        _: &Self::Caller,
        _: CallToolRequestParams,
    ) -> Result<Option<CreateTaskResult>, ErrorData> {
        Ok(None)
    }

    async fn get_task(
        &self,
        caller: &Self::Caller,
        request: GetTaskParams,
    ) -> Result<GetTaskResult, ErrorData> {
        get_durable_task(
            &self.runtime,
            &self.runtime.for_owner(&task_owner(caller)?),
            request,
        )
        .await
    }

    async fn update_task(
        &self,
        caller: &Self::Caller,
        request: UpdateTaskParams,
    ) -> Result<(), ErrorData> {
        update_durable_task(&self.runtime.for_owner(&task_owner(caller)?), request).await
    }

    async fn cancel_task(&self, caller: &Self::Caller, task_id: String) -> Result<(), ErrorData> {
        cancel_durable_task(&self.runtime.for_owner(&task_owner(caller)?), task_id).await
    }

    async fn subscribe_tasks(
        &self,
        caller: &Self::Caller,
        task_ids: Vec<String>,
    ) -> Result<DurableTaskSubscription, ErrorData> {
        subscribe_durable_tasks(
            &self.runtime,
            &self.runtime.for_owner(&task_owner(caller)?),
            task_ids,
        )
        .await
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[veoveo_hub_sdk::tool(provider=ForwarderProvider,output=Forwarded,name="forwarder_start")]
pub struct ForwarderStart {}

/// What Forwarder got when it called Spawner for its caller.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, veoveo_hub_sdk::SingleOutput)]
pub struct Forwarded {
    pub related_task: Option<String>,
    pub source: String,
}

#[derive(Clone)]
pub struct ForwarderDomain;

static FORWARDER_ROUTES: LazyLock<Routes<ForwarderDomain>> = LazyLock::new(|| {
    Routes::builder(RoutePolicy::Authenticated)
        .tool::<ForwarderStart, _, _>(
            "Have Spawner start a Task for the caller",
            RoutePolicy::permission(FixturePermission::CallsMake).unwrap(),
            |_: ForwarderDomain, _, context| async move { ForwarderDomain::start(context).await },
        )
        .uses(permission(FixturePermission::TasksStart))
        .build()
});
fixture!(
    "forwarder",
    ForwarderProvider,
    ForwarderContract,
    ForwarderDomain,
    FORWARDER_ROUTES
);

impl ForwarderDomain {
    async fn start(context: RequestContext<RoleServer>) -> Result<Forwarded, ToolError> {
        let mut arguments = rmcp::model::JsonObject::new();
        arguments.insert("naming".into(), json!("one"));
        let result = context
            .client()?
            .call_tool(
                HubToolName::parse("spawner__spawner_start").unwrap(),
                arguments,
            )
            .await?;
        let started: Started = serde_json::from_value(
            result
                .structured_content
                .clone()
                .ok_or_else(|| ToolError::Rejected("Spawner answered no Task".into()))?,
        )
        .map_err(|error| ToolError::Rejected(error.to_string()))?;
        Ok(Forwarded {
            related_task: related_task(&result).map(str::to_owned),
            source: started.source,
        })
    }
}

fn related_task(result: &CallToolResult) -> Option<&str> {
    result
        .meta
        .as_ref()?
        .get(RELATED_TASK_META_KEY)?
        .get("taskId")?
        .as_str()
}

/// Rewrites the related task of Spawner's answer to a `spawner_start` that asked for a
/// malformed naming, as a server that breaks the protocol would answer.
async fn misname(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let (mut parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let rewrite = |json: &str| -> Option<String> {
        let mut answer: Value = serde_json::from_str(json.trim()).ok()?;
        let result = answer.get_mut("result")?;
        let source = result["structuredContent"]["source"].as_str()?.to_owned();
        let related = match result["structuredContent"]["naming"].as_str()? {
            "malformed" => json!({ "taskId": 7 }),
            "two" => json!([{ "taskId": source }, { "taskId": source }]),
            _ => return None,
        };
        result["_meta"][RELATED_TASK_META_KEY] = related;
        Some(answer.to_string())
    };
    let rewritten: String = if text.lines().any(|line| line.starts_with("data:")) {
        text.lines()
            .map(|line| match line.strip_prefix("data:").and_then(rewrite) {
                Some(json) => format!("data: {json}"),
                None => line.to_owned(),
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    } else {
        rewrite(&text).unwrap_or(text)
    };
    parts.headers.remove(axum::http::header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(rewritten))
}

async fn store(registration: &veoveo_hub_sdk::registration::client::Registration) -> PlatformStore {
    let grant = registration.database().unwrap();
    PlatformStore::connect(
        StoreConfig::builder(grant.url.as_str(), grant.password.clone())
            .username(grant.username.as_str())
            .max_connections(4)
            .build()
            .unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn an_ordinary_call_that_starts_a_task_names_it_as_the_hub_records_it() {
    tokio::time::timeout(Duration::from_secs(180), qualify())
        .await
        .expect("call-started Tasks exceeded three minutes");
}

async fn qualify() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    let hub = Hub::start(directory.path(), &db, Duration::from_secs(5)).await;
    let http = reqwest::Client::new();

    let slug = ServerSlug::parse("spawner").unwrap();
    let mut instance = None;
    let spawner = serve_wrapped(
        &hub,
        &http,
        "spawner",
        SpawnerDomain::routes().authorization(),
        |router| router.layer(axum::middleware::from_fn(misname)),
        async |registration, metadata, _| {
            let store = store(registration).await;
            let started = Arc::new(Instance::start(&store, &slug).await.unwrap());
            instance = Some(started.clone());
            let runtime = TaskRuntime::new(
                store,
                slug.clone(),
                metadata.registration.deployment,
                started,
            );
            let domain = SpawnerDomain {
                runtime: runtime.clone(),
            };
            let tasks = SpawnerTasks { runtime };
            hosted::<SpawnerDomain>(registration, &http)
                .handler(move || {
                    Hosted::new(domain.clone()).with_tasks(DurableTasks::tasks_only(tasks.clone()))
                })
                .build()
        },
    )
    .await;
    let mut forwarder_store = None;
    let _forwarder = serve(
        &hub,
        &http,
        "forwarder",
        ForwarderDomain::routes().authorization(),
        async |registration, metadata, key| {
            forwarder_store = Some(store(registration).await);
            let calls = ServerCalls::discover(registration, metadata, key, http.clone())
                .await
                .unwrap();
            hosted::<ForwarderDomain>(registration, &http)
                .calls(calls)
                .handler(|| Hosted::new(ForwarderDomain))
                .build()
        },
    )
    .await;
    let member = mcp_client(&hub, hub.access_token(&http, hub.caller("member")).await).await;

    // A direct call: the result names the Task by the Hub's identifier, not Spawner's.
    let result = member
        .call_tool(
            CallToolRequestParams::new("spawner__spawner_start")
                .with_arguments(json!({"naming": "one"}).as_object().cloned().unwrap()),
        )
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true), "{result:?}");
    let started: Started =
        serde_json::from_value(result.structured_content.clone().unwrap()).unwrap();
    let canonical = related_task(&result)
        .unwrap_or_else(|| panic!("no related task: {result:?}"))
        .to_owned();
    assert_ne!(canonical, started.source);
    assert!(canonical.starts_with("gtr_"), "{canonical}");

    // Its route is a Task tool's: the caller's, with Spawner's time to live and the
    // caller it acts for while it runs.
    let (ttl_ms, delegates): (f64, bool) = sqlx::query_as(
        "SELECT extract(epoch FROM expires_at - created_at)::float8 * 1000, delegated_caller IS NOT NULL
         FROM hub.task_route WHERE server = 'spawner' AND source_task_id = $1",
    )
    .bind(&started.source)
    .fetch_one(db.a.pool())
    .await
    .unwrap();
    assert_eq!(ttl_ms.round() as u64, TASK_TTL_MS);
    assert!(delegates);

    // The person follows it by that identifier.
    let followed = member
        .get_task(GetTaskParams::new(&canonical))
        .await
        .unwrap()
        .task;
    assert_eq!(followed.task.task_id, canonical);
    assert!(
        matches!(followed.payload, TaskPayload::Working),
        "{followed:?}"
    );

    // The Task exchanges for its caller while it runs.
    let (status, exchanged) = exchange(
        &hub,
        &http,
        &spawner,
        (&started.source, TASK_TOKEN_TYPE),
        None,
        None,
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{exchanged}");
    let claims = decoded_claims(exchanged["access_token"].as_str().unwrap());
    let caller = decoded_claims(&spawner.bearers.last());
    let claim = "https://veoveo.ai/claims/invocation";
    // The caller of the call that started it, which the Task's route kept.
    assert_eq!(
        claims[claim]["caller"]["actor"]["id"], caller[claim]["actor"]["id"],
        "{claims}"
    );
    assert_eq!(
        claims[claim]["caller"]["client"], "acceptance-member",
        "{claims}"
    );
    assert_eq!(claims[claim]["acting_server"], "spawner", "{claims}");
    assert_eq!(claims[claim]["task"], started.source.as_str(), "{claims}");

    // A Task another server's call started is recorded, renamed, and delegates nothing.
    let forwarded = member
        .call_tool(
            CallToolRequestParams::new("forwarder__forwarder_start")
                .with_arguments(Default::default()),
        )
        .await
        .unwrap();
    assert_ne!(forwarded.is_error, Some(true), "{forwarded:?}");
    let forwarded: Forwarded =
        serde_json::from_value(forwarded.structured_content.unwrap()).unwrap();
    let renamed = forwarded
        .related_task
        .expect("Forwarder saw a related task");
    assert_ne!(renamed, forwarded.source);
    assert!(renamed.starts_with("gtr_"), "{renamed}");
    let delegates: bool = sqlx::query_scalar(
        "SELECT delegated_caller IS NOT NULL FROM hub.task_route
         WHERE server = 'spawner' AND source_task_id = $1",
    )
    .bind(&forwarded.source)
    .fetch_one(db.a.pool())
    .await
    .unwrap();
    assert!(!delegates);
    let (status, refused) = exchange(
        &hub,
        &http,
        &spawner,
        (&forwarded.source, TASK_TOKEN_TYPE),
        None,
        None,
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{refused}");
    assert_eq!(refused["error"], "invalid_grant", "{refused}");

    // A related task that is not one Task identifier is Spawner's protocol error.
    for naming in ["malformed", "two"] {
        let refused = member
            .call_tool(
                CallToolRequestParams::new("spawner__spawner_start")
                    .with_arguments(json!({"naming": naming}).as_object().cloned().unwrap()),
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains("not one Task identifier"),
            "{naming}: {refused}"
        );
    }
    drop(instance);
}
