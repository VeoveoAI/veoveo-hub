//! A linked server against the real Hub binary and SDK fixture. The base branch `dev` of
//! Ledger's synthetic repository is linked: two runners register with their instance
//! tokens, one for the link `team-a` and one for `team-b`, each of a synthetic linked
//! repository. The installation's Lore section grants the local `member` caller `write`
//! on team-a's repository alone, and the `admin` caller a read grant on both. No sandbox,
//! Lore server or application inference is involved; the runners are what the Hub
//! records when it creates them.
#[path = "support/hub.rs"]
mod hub;
#[path = "support/servers.rs"]
mod servers;

use std::time::Duration;

use hub::Hub;
use rmcp::{
    RoleClient,
    model::{
        CallToolRequestParams, CallToolResponse, ClientConfig, ClientRequest, GetTaskParams,
        ListResourceTemplatesRequest, ListToolsRequest, ReadResourceRequestParams,
        ResourceContents, ServerResult, TaskPayload,
    },
    service::RunningService,
};
use serde_json::{Value, json};
use servers::{Served, decoded_claims, hosted, mcp_client, serve_instance};
use veoveo_fixtures::store::TestDb;
use veoveo_hub::{
    data_environment::DatabaseName,
    execution::BranchName,
    factory::{Link, TrackedBranch},
    registration::create_runner,
};
use veoveo_hub_sdk::hub::LinkName;
use veoveo_hub_sdk_fixture::ledger::{self, LedgerDomain, LedgerTasks};

/// The parent repository, whose base branch is linked.
const REPOSITORY: &str = "0123456789abcdef0123456789abcdef";
const TEAM_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TEAM_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
/// How long one of these tests may take, Hub start included.
const BOUND: Duration = Duration::from_secs(120);
const BASE: &str = "dev";
const REPOSITORY_DETAIL: &str = "https://veoveo.ai/oauth/repository";

type Client = RunningService<RoleClient, ClientConfig>;

/// The Hub with its base branch linked, served by one runner of Ledger per link.
struct Fixture {
    hub: Hub,
    http: reqwest::Client,
    team_a: Served,
    team_b: Served,
    db: TestDb,
    _directory: tempfile::TempDir,
}

impl Fixture {
    async fn start() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let directory = tempfile::tempdir().unwrap();
        let db = TestDb::new().await;
        let hub = Hub::start_with(
            directory.path(),
            &db,
            Duration::from_secs(60),
            &[],
            |config| {
                let issuer = config["authorizationServers"][0]["issuer"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                let member = format!("{issuer}#acceptance-member");
                let admin = format!("{issuer}#acceptance-admin");
                config["execution"]["baseBranch"] = json!(BASE);
                config["lore"] = json!({
                    "deployments": [{
                        "id": "source", "resourceName": "native-source",
                        "audience": "https://lore.internal/native",
                        "endpoint": "grpcs://lore.internal:41337",
                        "authorizationServer": "local", "policyVersion": "local",
                        "loginScopes": ["native:login", "offline_access"],
                        "scopeTemplate": "source-{id}", "resourceIdTemplate": "{id}",
                        "tokenLifetimeSeconds": 120,
                    }],
                    "repositories": [
                        {"deployment": "source", "id": REPOSITORY, "tenant": "local", "title": "Ledger"},
                        {"deployment": "source", "id": TEAM_A, "tenant": "local", "title": "Team A"},
                        {"deployment": "source", "id": TEAM_B, "tenant": "local", "title": "Team B"},
                    ],
                    "assignments": [
                        {"deployment": "source", "repository": TEAM_A, "tenant": "local", "principal": member, "actions": ["write"]},
                        {"deployment": "source", "repository": TEAM_A, "tenant": "local", "principal": admin, "actions": []},
                        {"deployment": "source", "repository": TEAM_B, "tenant": "local", "principal": admin, "actions": []},
                    ],
                });
            },
        )
        .await;
        let http = reqwest::Client::new();
        let mut runners = Vec::new();
        for (link, linked) in [("team-a", TEAM_A), ("team-b", TEAM_B)] {
            let instance = create_runner(
                &db.a,
                REPOSITORY,
                &DatabaseName::installation(&db.a).unwrap(),
                &BranchName::parse(BASE).unwrap(),
                &Link {
                    name: LinkName::parse(link).unwrap(),
                    repository: linked.into(),
                    tracked: TrackedBranch {
                        id: "1".repeat(32),
                        name: BASE.into(),
                    },
                },
            )
            .await
            .unwrap();
            // One runner's state outlives each request's session.
            let tasks = LedgerTasks::new(Duration::from_secs(1));
            runners.push(
                serve_instance(
                    &hub,
                    &http,
                    "ledger",
                    ledger::authorization().unwrap(),
                    &instance,
                    async |registration, _, _| {
                        hosted::<LedgerDomain>(registration, &http)
                            .handler(move || ledger::hosted(tasks.clone()))
                            .build()
                    },
                )
                .await,
            );
        }
        let team_b = runners.pop().unwrap();
        let team_a = runners.pop().unwrap();
        Self {
            hub,
            http,
            team_a,
            team_b,
            db,
            _directory: directory,
        }
    }

    async fn client(&self, user: &str) -> Client {
        let token = self
            .hub
            .access_token(&self.http, self.hub.caller(user))
            .await;
        mcp_client(&self.hub, token).await
    }
}

/// The names of the tools the Hub lists to `client` now.
async fn tools(client: &Client) -> Vec<String> {
    let ServerResult::ListToolsResult(listed) = client
        .send_request(ClientRequest::ListToolsRequest(ListToolsRequest {
            method: Default::default(),
            params: None,
            extensions: Default::default(),
        }))
        .await
        .unwrap()
    else {
        panic!("tools/list returned another result");
    };
    let mut names = listed
        .tools
        .into_iter()
        .map(|tool| tool.name.into_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// The resource templates the Hub lists to `client` now.
async fn templates(client: &Client) -> Vec<String> {
    let ServerResult::ListResourceTemplatesResult(listed) = client
        .send_request(ClientRequest::ListResourceTemplatesRequest(
            ListResourceTemplatesRequest {
                method: Default::default(),
                params: None,
                extensions: Default::default(),
            },
        ))
        .await
        .unwrap()
    else {
        panic!("resources/templates/list returned another result");
    };
    listed
        .resource_templates
        .into_iter()
        .map(|template| template.uri_template)
        .filter(|template| template.starts_with("ledger://"))
        .collect()
}

fn call(name: &str, arguments: Value) -> CallToolRequestParams {
    CallToolRequestParams::new(name.to_owned())
        .with_arguments(arguments.as_object().unwrap().clone())
}

/// The repository entries of `authorization_details` in the last token `served` received.
fn repository_details(served: &Served) -> Vec<Value> {
    decoded_claims(&served.bearers.last())["authorization_details"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|detail| detail["type"] == REPOSITORY_DETAIL)
        .cloned()
        .collect()
}

#[tokio::test]
async fn each_link_serves_its_own_names_to_the_people_its_repository_grants() {
    tokio::time::timeout(BOUND, async {
        let fixture = Fixture::start().await;
        let (member, admin) = (
            fixture.client("member").await,
            fixture.client("admin").await,
        );

        // The member holds a grant on team-a's repository alone: they see team-a's
        // tools under its link, and no name without a link.
        let listed = tools(&member).await;
        assert!(
            listed.contains(&"ledger__team-a__inspect".to_owned()),
            "{listed:?}"
        );
        assert!(
            listed
                .iter()
                .all(|name| !name.starts_with("ledger__") || name.starts_with("ledger__team-a__")),
            "{listed:?}"
        );
        let both = tools(&admin).await;
        assert!(
            both.contains(&"ledger__team-a__inspect".to_owned())
                && both.contains(&"ledger__team-b__inspect".to_owned()),
            "{both:?}"
        );
        assert_eq!(
            templates(&member).await,
            [
                "ledger://team-a/docs/{doc_id}",
                "ledger://team-a/entries/{entry}"
            ]
        );

        // A call reaches its link's runner, which reads the caller's grant on its
        // repository from the forwarded token.
        let inspected = member
            .call_tool(call("ledger__team-a__inspect", json!({})))
            .await
            .unwrap();
        assert_ne!(inspected.is_error, Some(true), "{inspected:?}");
        assert_eq!(
            repository_details(&fixture.team_a),
            [json!({"type": REPOSITORY_DETAIL, "identifier": TEAM_A, "actions": ["write"]})]
        );
        admin
            .call_tool(call("ledger__team-b__inspect", json!({})))
            .await
            .unwrap();
        assert_eq!(
            repository_details(&fixture.team_b),
            [json!({"type": REPOSITORY_DETAIL, "identifier": TEAM_B, "actions": []})]
        );

        // A link the caller holds no grant on, and a name without a link, are tools the
        // server did not declare.
        for refused in [
            "ledger__team-b__inspect",
            "ledger__team-c__inspect",
            "ledger__inspect",
        ] {
            let error = member
                .call_tool(call(refused, json!({})))
                .await
                .unwrap_err()
                .to_string();
            // The refusal names the tool as the caller wrote it.
            assert!(
                error.contains(&format!("You don't have permission to call `{refused}`."))
                    && error.contains("isn't available in this profile"),
                "{refused}: {error}"
            );
        }

        // A runner's resources carry its link after the scheme, and reach it without.
        member
            .call_tool(call(
                "ledger__team-a__open_entry",
                json!({"entry": "first", "close_after_ms": 60_000}),
            ))
            .await
            .unwrap();
        let read = member
            .read_resource(ReadResourceRequestParams::new(
                "ledger://team-a/entries/first",
            ))
            .await
            .unwrap();
        let [ResourceContents::TextResourceContents { uri, .. }] = read.contents.as_slice() else {
            panic!("unexpected contents: {:?}", read.contents);
        };
        assert_eq!(uri, "ledger://team-a/entries/first");
        let refused = member
            .read_resource(ReadResourceRequestParams::new(
                "ledger://team-b/entries/first",
            ))
            .await
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains("read `ledger://team-b/entries/first`")
                && refused.contains("isn't available in this profile"),
            "{refused}"
        );

        // A Task keeps the runner that created it.
        let CallToolResponse::Task(created) = member
            .call_tool_once(call("ledger__team-a__tally", json!({"entries": 3})))
            .await
            .unwrap()
        else {
            panic!("team-a's runner did not create a Task");
        };
        let calls_to_b = fixture.team_b.bearers.snapshot().len();
        let result = loop {
            match member
                .get_task(GetTaskParams::new(&created.task.task_id))
                .await
                .unwrap()
                .task
                .payload
            {
                TaskPayload::Completed { result } => break result,
                TaskPayload::Failed { error } => panic!("the Task failed: {error:?}"),
                _ => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        };
        assert_eq!(result["structuredContent"]["entries"], 3);
        assert_eq!(fixture.team_b.bearers.snapshot().len(), calls_to_b);

        // The audit records name each target as its caller wrote it: a runner's tools
        // with their link and its resources under it, never the runner's own names.
        let targets: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT draft ->> 'target' FROM audit.record \
             WHERE draft ->> 'target' LIKE 'veoveo://servers/ledger/%' \
             OR draft ->> 'target' LIKE 'ledger://%'",
        )
        .fetch_all(fixture.db.a.pool())
        .await
        .unwrap();
        for written in [
            "veoveo://servers/ledger/links/team-a/tools/inspect",
            "veoveo://servers/ledger/links/team-b/tools/inspect",
            "veoveo://servers/ledger/links/team-c/tools/inspect",
            "veoveo://servers/ledger/links/team-a/tools/tally",
            "ledger://team-a/entries/first",
            "ledger://team-b/entries/first",
        ] {
            assert!(
                targets.iter().any(|target| target == written),
                "{written}: {targets:?}"
            );
        }
        assert!(
            !targets
                .iter()
                .any(|target| target.starts_with("ledger://entries/")),
            "{targets:?}"
        );
        member.cancel().await.unwrap();
        admin.cancel().await.unwrap();
    })
    .await
    .expect("the test exceeded its bound");
}
