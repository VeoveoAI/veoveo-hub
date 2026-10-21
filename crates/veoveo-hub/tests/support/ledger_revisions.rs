//! Synthetic source revisions of the existing Ledger fixture. Both use its typed
//! call/result contract and Task implementation, but advertise distinct descriptors.
//! This tests discovery caches without rewriting MCP responses or involving live data.
use rmcp::model::ServerConfig;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use veoveo_hub_sdk::{
    docs::ServerDocs,
    hosting::{ClientContext, DomainServer, RoutePolicy, Routes, hub_identity},
    server_contract::{McpServerContract, McpServerSetup},
};
use veoveo_hub_sdk_fixture::{
    calls::{Inspect, Inspection, Ledger, LedgerPermission, Tallied, Tally},
    ledger::{self, LedgerContract},
};
use veoveo_types::ResourceScheme;

/// Source revision B adds a real optional typed argument/result, while the
/// inherited Relay's existing empty Inspect arguments keep their original shape.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SecondInspect {
    #[serde(default)]
    pub probe: Option<String>,
}
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct SecondInspection {
    #[serde(flatten)]
    pub inspection: Inspection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe: Option<String>,
}
impl veoveo_hub_sdk::definition::SingleOutput for SecondInspection {}
impl veoveo_hub_sdk::calls::ToolCall for SecondInspect {
    type Provider = Ledger;
    type Output = SecondInspection;
    const TOOL: &'static str = "inspect";
}
trait InspectRevision: veoveo_hub_sdk::calls::ToolCall {
    fn answer(self, inspection: Inspection) -> Self::Output;
}
impl InspectRevision for Inspect {
    fn answer(self, inspection: Inspection) -> Self::Output {
        inspection
    }
}
impl InspectRevision for SecondInspect {
    fn answer(self, inspection: Inspection) -> Self::Output {
        SecondInspection {
            inspection,
            probe: self.probe,
        }
    }
}

macro_rules! revision {
    ($domain:ident, $contract:ident, $inspect:ty, $description:literal) => {
        #[derive(Clone)]
        pub struct $domain;
        pub struct $contract;
        impl McpServerContract for $contract {
            type Server = $domain;
            type Scope = <LedgerContract as McpServerContract>::Scope;
            type Provider = Ledger;
            fn scheme() -> ResourceScheme {
                LedgerContract::scheme()
            }
            fn scopes() -> &'static [Self::Scope] {
                LedgerContract::scopes()
            }
            fn documents() -> &'static ServerDocs {
                LedgerContract::documents()
            }
            fn server_config() -> ServerConfig {
                LedgerContract::server_config()
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
                static ROUTES: LazyLock<Routes<$domain>> = LazyLock::new(|| {
                    Routes::builder(RoutePolicy::Authenticated)
                        .tool::<$inspect, _, _>(
                            $description,
                            RoutePolicy::permission(LedgerPermission::EntriesRead).unwrap(),
                            |_, request, context| async move {
                                Ok(request.answer(ledger::inspection(&hub_identity(&context)?)))
                            },
                        )
                        .task::<Tally, _, _>(
                            "Count entries",
                            RoutePolicy::permission(LedgerPermission::EntriesRead).unwrap(),
                            |_, request, context| async move {
                                Ok(Tallied {
                                    entries: request.entries,
                                    inspection: ledger::inspection(&hub_identity(&context)?),
                                })
                            },
                        )
                        .build()
                });
                &ROUTES
            }
        }
    };
}
revision!(
    FirstRevision,
    FirstContract,
    Inspect,
    "Inspect identity from source revision A"
);
revision!(
    SecondRevision,
    SecondContract,
    SecondInspect,
    "Inspect identity from source revision B"
);

/// This scenario needs only inspection delegation. Its actual typed route and
/// registration both declare exactly that dependency; no metadata is trimmed
/// after construction while unrelated Relay tools remain served.
#[derive(Clone)]
pub struct InspectionRelay;
pub struct InspectionRelayContract;
impl McpServerContract for InspectionRelayContract {
    type Server = InspectionRelay;
    type Scope = <veoveo_hub_sdk_fixture::relay::RelayContract as McpServerContract>::Scope;
    type Provider = veoveo_hub_sdk_fixture::relay::RelayContract;
    fn scheme() -> ResourceScheme {
        Self::Provider::scheme()
    }
    fn scopes() -> &'static [Self::Scope] {
        Self::Provider::scopes()
    }
    fn documents() -> &'static ServerDocs {
        Self::Provider::documents()
    }
    fn server_config() -> ServerConfig {
        Self::Provider::server_config()
    }
}
impl DomainServer for InspectionRelay {
    type Contract = InspectionRelayContract;
    fn setup() -> &'static McpServerSetup<Self::Contract> {
        static SETUP: LazyLock<McpServerSetup<InspectionRelayContract>> =
            LazyLock::new(|| McpServerSetup::new().unwrap());
        &SETUP
    }
    fn routes() -> &'static Routes<Self> {
        static ROUTES: LazyLock<Routes<InspectionRelay>> = LazyLock::new(|| {
            Routes::builder(RoutePolicy::Authenticated)
                .tool::<veoveo_hub_sdk_fixture::relay::InspectRelay, _, _>(
                    "Relay identity",
                    RoutePolicy::permission(
                        veoveo_hub_sdk_fixture::calls::RelayPermission::CallsMake,
                    )
                    .unwrap(),
                    |_, _, context| async move {
                        context.client()?.call(Inspect {}).await.map_err(Into::into)
                    },
                )
                .uses(veoveo_types::PermissionName::parse("ledger.entries.read").unwrap())
                .build()
        });
        &ROUTES
    }
}
