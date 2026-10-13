//! Calls between servers through the Gateway binary: the SDK's Relay fixture calls its
//! Ledger fixture for Relay's caller with an RFC 8693 exchanged token, inline, through a
//! Ledger Task and from a durable Relay Task after the call that created it returned.
//! The Gateway narrows each call to the uses Relay declared and the caller holds, names
//! a missing provider, and refuses another server's subject, a delegated subject and an
//! ended Task.

#[path = "support/gateway.rs"]
mod gateway;

use std::{
    net::Ipv4Addr,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use gateway::Gateway;
use rmcp::{
    ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::{
        CallToolRequestParams, CallToolResponse, ClientCapabilities, ClientConfig, GetTaskParams,
        Implementation, ProtocolVersion, TaskPayload,
    },
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use veoveo_platform_store::{Instance, PlatformStore, StoreConfig};
use veoveo_sdk_rust::{
    GatewayProfileId,
    calls::{TokenErrorCode, ToolCall},
    client::CallError,
    hosting::{HostedServer, ServerCalls},
    registration::{
        AuthorizationDeclaration, ClientMetadata, RegistrationDeclaration, ServerEndpoint,
        ServerVersion,
        client::{Registration, RegistrationClient},
        statement::ServerKeyPair,
    },
};
use veoveo_sdk_rust_fixture::{
    calls::{EntryState, FollowedEntry, Inspect, Inspection},
    ledger::{self, LedgerDomain, LedgerTasks},
    relay::{self, RelayDomain, RelayTasks},
};
use veoveo_task_runtime::TaskRuntime;
use veoveo_test_fixtures::store::TestDb;
use veoveo_types::{PermissionName, ResourceScheme, ServerSlug, TaskId};

const PROFILE: &str = "local";

#[tokio::test]
async fn servers_call_each_other_for_their_caller_through_the_gateway() {
    tokio::time::timeout(Duration::from_secs(240), qualify())
        .await
        .expect("calls between servers exceeded four minutes");
}

/// The bearer tokens a fixture server received.
#[derive(Clone, Default)]
struct Bearers(Arc<Mutex<Vec<String>>>);

impl Bearers {
    fn last(&self) -> String {
        self.0
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("a forwarded token arrived")
    }
}

async fn record(State(bearers): State<Bearers>, request: Request, next: Next) -> Response {
    if let Some(bearer) = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    {
        bearers.0.lock().unwrap().push(bearer.to_owned());
    }
    next.run(request).await
}

/// One fixture server: its key, registration and the bearers it received.
struct Served {
    key: ServerKeyPair,
    metadata: ClientMetadata,
    client: RegistrationClient,
    registration: Registration,
    bearers: Bearers,
}

/// Register `declaration` under `slug` with a server listening on a loopback port, and
/// serve what `build` makes of its registration.
async fn serve(
    gateway: &Gateway,
    http: &reqwest::Client,
    slug: &str,
    declaration: AuthorizationDeclaration,
    build: impl AsyncFnOnce(&Registration, &ClientMetadata, &ServerKeyPair) -> HostedServer,
) -> Served {
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let key = ServerKeyPair::generate().unwrap();
    let metadata = ClientMetadata::new(
        *key.public_key(),
        RegistrationDeclaration {
            slug: ServerSlug::parse(slug).unwrap(),
            uri_scheme: ResourceScheme::parse(slug).unwrap(),
            version: ServerVersion::new("0.1.0").unwrap(),
            endpoint: ServerEndpoint::new(
                url::Url::parse(&format!("http://{address}/{slug}/mcp")).unwrap(),
            )
            .unwrap(),
        },
        declaration,
    )
    .unwrap();
    let issuer = url::Url::parse(&gateway.base).unwrap();
    let endpoint = RegistrationClient::discover(http, &issuer).await.unwrap();
    let client = RegistrationClient::new(
        http.clone(),
        endpoint,
        SecretString::from(gateway.secret("VEOVEO_REGISTRATION_SECRET").to_owned()),
        ServerKeyPair::from_pkcs8_der(key.pkcs8_der()).unwrap(),
        metadata.clone(),
    )
    .unwrap();
    let registration = client.register().await.unwrap();
    let bearers = Bearers::default();
    let router: Router = build(&registration, &metadata, &key)
        .await
        .into_router()
        .layer(axum::middleware::from_fn_with_state(
            bearers.clone(),
            record,
        ));
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    Served {
        key,
        metadata,
        client,
        registration,
        bearers,
    }
}

fn hosted<D: veoveo_sdk_rust::hosting::DomainServer>(
    registration: &Registration,
    http: &reqwest::Client,
) -> veoveo_sdk_rust::hosting::HostedServerBuilder<
    D,
    veoveo_sdk_rust::hosting::Provided<veoveo_sdk_rust::hosting::Deployment>,
    veoveo_sdk_rust::hosting::Provided<Arc<veoveo_sdk_rust::ForwardedTokenVerifier>>,
    veoveo_sdk_rust::hosting::Missing,
> {
    let endpoint = registration
        .information()
        .metadata
        .registration
        .endpoint
        .url()
        .clone();
    let authority = format!(
        "{}:{}",
        endpoint.host_str().unwrap(),
        endpoint.port().unwrap()
    );
    HostedServer::for_domain::<D>()
        .internal([authority])
        .internal_trust(registration.access_tokens(), http.clone())
        .unwrap()
}

async fn caller(
    gateway: &Gateway,
    http: &reqwest::Client,
    role: &str,
) -> RunningService<RoleClient, ClientConfig> {
    let token = gateway.access_token(http, gateway.caller(role)).await;
    ClientConfig::new(
        ClientCapabilities::builder().enable_tasks().build(),
        Implementation::new("calls-qualification", "0.1.0"),
    )
    .serve_with_lifecycle(
        StreamableHttpClientTransport::from_config(
            StreamableHttpClientTransportConfig::with_uri(gateway.installation.resource.clone())
                .auth_header(token),
        ),
        ClientLifecycleMode::Discover {
            preferred_versions: vec![ProtocolVersion::V_2026_07_28],
        },
    )
    .await
    .unwrap()
}

/// Relay's answer to `tool`: its structured result, or the error it reported.
async fn relay_call(
    client: &RunningService<RoleClient, ClientConfig>,
    tool: &str,
    arguments: Value,
) -> Result<Value, String> {
    let request = CallToolRequestParams::new(format!("relay__{tool}"))
        .with_arguments(arguments.as_object().cloned().unwrap());
    let result = client
        .call_tool(request)
        .await
        .map_err(|error| error.to_string())?;
    if result.is_error == Some(true) {
        return Err(result
            .content
            .iter()
            .filter_map(|content| content.as_text().map(|text| text.text.clone()))
            .collect::<Vec<_>>()
            .join("\n"));
    }
    result.structured_content.ok_or_else(|| "no result".into())
}

/// An RFC 8693 exchange `server` sends itself, with its own key.
async fn exchange(
    gateway: &Gateway,
    http: &reqwest::Client,
    server: &Served,
    subject: (&str, &str),
    details: Option<Value>,
    resource: Option<&str>,
) -> (reqwest::StatusCode, Value) {
    let token_endpoint = gateway.token_endpoint();
    let assertion = server
        .key
        .client_assertion(
            server.registration.client_id(),
            &token_endpoint,
            chrono::Utc::now(),
        )
        .unwrap();
    let mut form = vec![
        (
            "grant_type",
            veoveo_sdk_rust::calls::TOKEN_EXCHANGE_GRANT_TYPE.to_owned(),
        ),
        ("client_id", server.registration.client_id().to_string()),
        (
            "client_assertion_type",
            veoveo_sdk_rust::registration::assertion::CLIENT_ASSERTION_TYPE.to_owned(),
        ),
        ("client_assertion", assertion.expose_secret().to_owned()),
        ("subject_token", subject.0.to_owned()),
        ("subject_token_type", subject.1.to_owned()),
        (
            "resource",
            resource
                .unwrap_or(&gateway.installation.resource)
                .to_owned(),
        ),
    ];
    if let Some(details) = details {
        form.push(("authorization_details", details.to_string()));
    }
    let mut body = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in &form {
        body.append_pair(name, value);
    }
    let response = http
        .post(token_endpoint)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body.finish())
        .send()
        .await
        .unwrap();
    let status = response.status();
    (status, response.json().await.unwrap())
}

fn decoded_claims(token: &str) -> Value {
    use base64::Engine as _;
    serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(token.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap()
}

async fn qualify() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let db = TestDb::new().await;
    let gateway = Gateway::start(directory.path(), &db, Duration::from_secs(5)).await;
    let http = reqwest::Client::new();
    let stop = CancellationToken::new();
    let _stop = stop.clone().drop_guard();

    // Ledger declares a use of Relay's permission too, so it can try to pass on a
    // delegated identity it received.
    let mut ledger_declaration = ledger::authorization().unwrap();
    ledger_declaration
        .uses
        .push(PermissionName::parse("relay.calls.make").unwrap());
    let ledger_tasks = LedgerTasks::default();
    let ledger = serve(
        &gateway,
        &http,
        "ledger",
        ledger_declaration,
        async |registration, _, _| {
            let tasks = ledger_tasks.clone();
            hosted::<LedgerDomain>(registration, &http)
                .handler(move || ledger::hosted(tasks.clone()))
                .build()
        },
    )
    .await;

    // Relay runs `relay_later` as a durable Task in core's Task runtime, on the login its
    // registration grants.
    let relay_slug = ServerSlug::parse("relay").unwrap();
    let relay_calls: Arc<Mutex<Option<ServerCalls>>> = Arc::default();
    let relay = serve(
        &gateway,
        &http,
        "relay",
        relay::authorization().unwrap(),
        async |registration, metadata, key| {
            let calls = ServerCalls::discover(registration, metadata, key, http.clone())
                .await
                .unwrap();
            *relay_calls.lock().unwrap() = Some(calls.clone());
            let grant = registration.database().unwrap();
            let store = PlatformStore::connect(
                StoreConfig::builder(grant.url.as_str(), grant.password.clone())
                    .username(grant.username.as_str())
                    .max_connections(4)
                    .build()
                    .unwrap(),
            )
            .await
            .unwrap();
            let instance = Arc::new(Instance::start(&store, &relay_slug).await.unwrap());
            let tasks = RelayTasks::new(
                TaskRuntime::new(store, relay_slug.clone(), instance),
                calls.clone(),
            );
            tokio::spawn(tasks.clone().work(stop.child_token()));
            hosted::<RelayDomain>(registration, &http)
                .calls(calls)
                .handler(move || relay::hosted_with_tasks(tasks.clone()))
                .build()
        },
    )
    .await;
    let relay_calls = relay_calls.lock().unwrap().clone().unwrap();
    let member = caller(&gateway, &http, "member").await;
    let admin = caller(&gateway, &http, "admin").await;

    // Inline: Ledger sees the member as the caller and Relay as the acting server, with
    // only the Ledger permissions the member holds.
    let inspection: Inspection = serde_json::from_value(
        relay_call(&member, "relay_inspect", json!({}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(inspection.client, "acceptance-member");
    assert_eq!(inspection.acting_server.as_deref(), Some("relay"));
    assert_eq!(inspection.acting_task, None);
    assert_eq!(
        inspection.permissions,
        ["ledger.entries.read", "ledger.entries.write"]
    );
    let delegated = decoded_claims(&ledger.bearers.last());
    assert_eq!(delegated["act"]["sub"], gateway.base);
    assert_eq!(
        delegated["act"]["act"]["sub"],
        relay.registration.client_id().as_str()
    );
    assert_eq!(
        delegated["https://veoveo.ai/claims/invocation"]["acting_server"],
        "relay"
    );

    // Narrowing: the member holds no sensitive permission, the admin does.
    let refused = relay_call(&member, "relay_seal", json!({}))
        .await
        .unwrap_err();
    assert!(refused.contains("rejected"), "{refused}");
    let seal = relay_call(&admin, "relay_seal", json!({})).await.unwrap();
    assert_eq!(seal["value"], "seal-7");

    // A Ledger Task through the Gateway resolves Relay's Task handle.
    let tallied = relay_call(&member, "relay_tally", json!({"entries": 2}))
        .await
        .unwrap();
    assert_eq!(tallied["entries"], 2);
    assert_eq!(tallied["inspection"]["acting_server"], "relay");

    // Relay reads a Ledger entry through the Gateway and follows its updates through a
    // resource subscription until it closes.
    let followed: FollowedEntry = serde_json::from_value(
        relay_call(&member, "relay_follow_entry", json!({"entry": "gateway-1"}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(followed.states, [EntryState::Open, EntryState::Closed]);
    assert_eq!(followed.waited.state, EntryState::Closed);
    assert_eq!(followed.read, followed.waited);

    // A durable Relay Task calls Ledger after the call that created it returned.
    let created = match member
        .call_tool_once(
            CallToolRequestParams::new("relay__relay_later").with_arguments(Default::default()),
        )
        .await
        .unwrap()
    {
        CallToolResponse::Task(created) => created.task.task_id,
        other => panic!("relay_later did not start a Task: {other:?}"),
    };
    let later = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let task = member
                .get_task(GetTaskParams::new(&created))
                .await
                .unwrap()
                .task;
            match task.payload {
                TaskPayload::Completed { result } => return result,
                TaskPayload::Failed { error } => panic!("relay_later failed: {error:?}"),
                _ => tokio::time::sleep(Duration::from_millis(200)).await,
            }
        }
    })
    .await
    .expect("relay_later finished");
    let later: Inspection = serde_json::from_value(later["structuredContent"].clone()).unwrap();
    assert_eq!(later.client, "acceptance-member");
    assert_eq!(later.acting_server.as_deref(), Some("relay"));
    // Ledger sees the Relay Task the call came from, by the identifier Relay issued.
    let source: String = sqlx::query_scalar(
        "SELECT source_task_id FROM gateway.task_route WHERE server = 'relay' AND delegated_caller IS NOT NULL",
    )
    .fetch_one(db.a.pool())
    .await
    .unwrap();
    assert_eq!(later.acting_task.as_deref(), Some(source.as_str()));
    let from_task = decoded_claims(&ledger.bearers.last());
    assert_eq!(
        from_task["https://veoveo.ai/claims/invocation"]["acting_task"],
        source.as_str()
    );

    // Once the Task ended, it acts for nobody.
    let profile = GatewayProfileId::parse(PROFILE).unwrap();
    let ended = relay_calls
        .for_task_id(TaskId::parse(source.clone()).unwrap(), &profile)
        .call(Inspect {})
        .await;
    assert!(
        matches!(
            ended,
            Err(CallError::Refused {
                error: TokenErrorCode::InvalidGrant,
                ..
            })
        ),
        "{ended:?}"
    );

    // Refusals at the token endpoint, with Relay's own assertion.
    let access = veoveo_sdk_rust::calls::ACCESS_TOKEN_TOKEN_TYPE;
    relay_call(&member, "relay_inspect", json!({}))
        .await
        .unwrap();
    let relay_token = relay.bearers.last();
    let ledger_token = ledger.bearers.last();
    let undeclared = json!([{"type": "https://veoveo.ai/oauth/permissions", "actions": ["observatory.readings.read"]}]);
    let (status, answer) = exchange(
        &gateway,
        &http,
        &relay,
        (&relay_token, access),
        Some(undeclared),
        None,
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
    assert_eq!(answer["error"], "invalid_authorization_details", "{answer}");
    let (_, answer) = exchange(
        &gateway,
        &http,
        &relay,
        (&relay_token, access),
        None,
        Some("http://127.0.0.1:1/mcp/elsewhere"),
    )
    .await;
    assert_eq!(answer["error"], "invalid_target", "{answer}");
    // Another server's forwarded token is no subject of Relay's.
    let (_, answer) = exchange(&gateway, &http, &relay, (&ledger_token, access), None, None).await;
    assert_eq!(answer["error"], "invalid_grant", "{answer}");
    // A delegated identity is not delegated again.
    let (_, answer) = exchange(
        &gateway,
        &http,
        &ledger,
        (&ledger_token, access),
        None,
        None,
    )
    .await;
    assert_eq!(answer["error"], "invalid_grant", "{answer}");
    assert!(
        answer["error_description"]
            .as_str()
            .unwrap()
            .contains("delegated again"),
        "{answer}"
    );
    // Relay's own subject exchanges; the response names what the token carries: the
    // member's Ledger permissions among Relay's uses, without the sensitive seal.
    let (status, answer) =
        exchange(&gateway, &http, &relay, (&relay_token, access), None, None).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{answer}");
    assert_eq!(
        answer["authorization_details"][0]["actions"],
        json!(["ledger.entries.read", "ledger.entries.write"])
    );

    // Each exchange and each delegated admission is in the audit chain.
    let exchanges: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit.record WHERE class = 'gateway.token_exchange'",
    )
    .fetch_one(db.a.pool())
    .await
    .unwrap();
    assert!(exchanges >= 6, "{exchanges} exchange records");
    let acting: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit.record WHERE class = 'mcp.tool_admission' AND draft #>> '{via,server}' = 'relay'",
    )
    .fetch_one(db.a.pool())
    .await
    .unwrap();
    assert!(acting >= 3, "{acting} delegated admissions");
    let from_tasks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit.record WHERE class IN ('mcp.tool_admission', 'mcp.tool_completion') AND draft #>> '{via,task}' = $1",
    )
    .bind(&source)
    .fetch_one(db.a.pool())
    .await
    .unwrap();
    assert_eq!(
        from_tasks, 2,
        "the Task's call is admitted and completed once"
    );

    // Without Ledger, the refusal names the permission no server provides.
    ledger.client.deregister(ledger.registration).await.unwrap();
    let missing = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match relay_call(&member, "relay_inspect", json!({})).await {
                Err(message) if message.contains("no registered server provides") => {
                    return message;
                }
                _ => tokio::time::sleep(Duration::from_millis(200)).await,
            }
        }
    })
    .await
    .expect("the Gateway named the missing provider");
    assert!(missing.contains("ledger.entries.read"), "{missing}");
    let _ = (
        Inspect::TOOL,
        &ledger.metadata,
        &relay.metadata,
        &relay.client,
    );
    member.cancel().await.unwrap();
    admin.cancel().await.unwrap();
    drop(gateway);
}
