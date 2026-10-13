//! The usage views the Gateway serves: daily totals from core's usage ledger across
//! every registered server, for the caller alone or for the caller's tenant.
use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{PrincipalId, ServerSlug};
use veoveo_usage_contract::{Money, Quantity, UsageKind, UsagePhase};

/// Whose usage a view reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UsageScope {
    /// The caller's own usage in its tenant.
    #[default]
    Own,
    /// Every principal's usage in the caller's tenant, for the tenant's administrators.
    Tenant,
}

/// One day's totals of one kind for one principal, from one server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DailyUsageRow {
    pub day: NaiveDate,
    pub principal: PrincipalId,
    /// The server whose kind this is.
    pub server: ServerSlug,
    pub kind: UsageKind,
    pub phase: UsagePhase,
    pub quantity: Quantity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub money: Option<Money>,
    /// How many ledger records the totals sum.
    pub records: u64,
}

/// One page of daily totals and the cursor of the next, when there is one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsagePage {
    pub rows: Vec<DailyUsageRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
