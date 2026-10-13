//! The installation's usage across servers: pages of daily totals from core's usage
//! ledger for one tenant, and the monthly record partitions the ledger writes into.
use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Months, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use veoveo_types::{CursorCodec, PrincipalId, ServerSlug, TenantId};
use veoveo_usage::UsageStore;
use veoveo_usage_contract::{Amount, Currency, Money, Quantity, UsageKind, UsagePhase};

use crate::GatewayState;
use crate::contract::{DailyUsageRow, UsageScope};

/// How many months ahead the Gateway keeps the ledger's record partitions.
pub const USAGE_PARTITION_MONTHS: u32 = 3;
/// The most rows one page of daily totals holds.
pub const MAX_USAGE_PAGE: u16 = 1000;
/// The currency the ledger stores for usage that carries no money.
const NO_CURRENCY: &str = "XXX";

/// Where a page of daily totals ends: the key of its last row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyUsagePosition {
    pub day: NaiveDate,
    pub principal: PrincipalId,
    pub server: ServerSlug,
    pub kind: UsageKind,
    pub phase: UsagePhase,
    /// The currency of the row's money; none for usage that carries no money.
    pub currency: Option<Currency>,
}

impl DailyUsagePosition {
    /// The currency key the ledger stores, which is `XXX` for usage without money.
    fn currency_key(&self) -> &str {
        self.currency.as_ref().map_or(NO_CURRENCY, Currency::as_str)
    }
}

/// One page of a tenant's daily totals.
#[derive(Debug, Clone)]
pub struct DailyUsageQuery {
    pub tenant: TenantId,
    /// Only this principal's totals, when given.
    pub principal: Option<PrincipalId>,
    /// The first day read, and the day after the last.
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub after: Option<DailyUsagePosition>,
    pub limit: u16,
}

impl GatewayState {
    /// Create the usage ledger's monthly record partitions through
    /// [`USAGE_PARTITION_MONTHS`] months from today. Returns how many it created.
    pub async fn prepare_usage_partitions(&self) -> Result<u32> {
        let through = Utc::now()
            .date_naive()
            .checked_add_months(Months::new(USAGE_PARTITION_MONTHS))
            .ok_or_else(|| anyhow!("the partition horizon is past the calendar"))?;
        UsageStore::new(self.platform_store().clone())
            .prepare_partitions(through)
            .await
            .context("failed to prepare the usage ledger's partitions")
    }

    /// One page of daily totals and, when more follow, the position after it.
    pub async fn daily_usage(
        &self,
        query: &DailyUsageQuery,
    ) -> Result<(Vec<DailyUsageRow>, Option<DailyUsagePosition>)> {
        let limit = query.limit.clamp(1, MAX_USAGE_PAGE);
        let after = query.after.as_ref();
        let rows = sqlx::query_file!(
            "src/usage/daily.sql",
            query.tenant.as_str(),
            query.principal.as_ref().map(PrincipalId::as_str),
            query.from,
            query.to,
            after.map(|after| after.day),
            after.map(|after| after.principal.as_str()),
            after.map(|after| after.server.as_str()),
            after.map(|after| after.kind.as_str()),
            after.map(|after| after.phase.as_str()),
            after.map(DailyUsagePosition::currency_key),
            i64::from(limit) + 1,
        )
        .fetch_all(self.platform_store().pool())
        .await
        .context("failed to read daily usage")?;
        let mut page = rows
            .into_iter()
            .map(|row| {
                let currency = (row.currency != NO_CURRENCY)
                    .then(|| Currency::parse(row.currency).context("stored currency"))
                    .transpose()?;
                let money = currency
                    .as_ref()
                    .map(|currency| {
                        Ok::<_, anyhow::Error>(Money {
                            amount: Amount::parse(row.amount).context("stored amount")?,
                            currency: currency.clone(),
                        })
                    })
                    .transpose()?;
                Ok((
                    DailyUsageRow {
                        day: row.day,
                        principal: PrincipalId::parse(row.principal).context("stored principal")?,
                        server: ServerSlug::parse(row.source).context("stored server")?,
                        kind: UsageKind::parse(row.kind).context("stored kind")?,
                        phase: row.phase.parse().context("stored phase")?,
                        quantity: Quantity::parse(row.quantity).context("stored quantity")?,
                        money,
                        records: u64::try_from(row.records).context("stored record count")?,
                    },
                    currency,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let next = if page.len() > usize::from(limit) {
            page.truncate(usize::from(limit));
            page.last().map(|(row, currency)| DailyUsagePosition {
                day: row.day,
                principal: row.principal.clone(),
                server: row.server.clone(),
                kind: row.kind.clone(),
                phase: row.phase,
                currency: currency.clone(),
            })
        } else {
            None
        };
        Ok((page.into_iter().map(|(row, _)| row).collect(), next))
    }
}

/// The longest cursor a usage view accepts, in bytes.
const MAX_CURSOR_BYTES: usize = 4096;

/// The opaque continuation of one usage view: the position after the last row, bound to
/// the view's scope, principal and days, so a cursor continues only the view it came
/// from.
pub struct DailyUsageCursor {
    pub scope: UsageScope,
    pub principal: Option<PrincipalId>,
    pub from: NaiveDate,
    pub to: NaiveDate,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorWire {
    scope: UsageScope,
    principal: Option<PrincipalId>,
    from: NaiveDate,
    to: NaiveDate,
    after: DailyUsagePosition,
}

impl CursorCodec for DailyUsageCursor {
    type Position = DailyUsagePosition;
    type Error = &'static str;

    fn check(&self, position: &DailyUsagePosition) -> Result<(), Self::Error> {
        if position.day < self.from || position.day >= self.to {
            return Err("the cursor lies outside the requested days");
        }
        if self
            .principal
            .as_ref()
            .is_some_and(|principal| principal != &position.principal)
        {
            return Err("the cursor belongs to another principal");
        }
        Ok(())
    }

    fn encode(&self, position: &DailyUsagePosition) -> Result<String, Self::Error> {
        let bytes = serde_json::to_vec(&CursorWire {
            scope: self.scope,
            principal: self.principal.clone(),
            from: self.from,
            to: self.to,
            after: position.clone(),
        })
        .map_err(|_| "the cursor cannot be encoded")?;
        let encoded = URL_SAFE_NO_PAD.encode(bytes);
        if encoded.len() > MAX_CURSOR_BYTES {
            return Err("the position exceeds the cursor limit");
        }
        Ok(encoded)
    }

    fn decode(&self, wire: &str) -> Result<DailyUsagePosition, Self::Error> {
        if wire.len() > MAX_CURSOR_BYTES {
            return Err("the cursor exceeds its limit");
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(wire)
            .map_err(|_| "the cursor is malformed")?;
        let cursor: CursorWire =
            serde_json::from_slice(&bytes).map_err(|_| "the cursor is malformed")?;
        if cursor.scope != self.scope
            || cursor.principal != self.principal
            || cursor.from != self.from
            || cursor.to != self.to
        {
            return Err("the cursor belongs to another view");
        }
        self.check(&cursor.after)?;
        Ok(cursor.after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codec(scope: UsageScope) -> DailyUsageCursor {
        DailyUsageCursor {
            scope,
            principal: Some(PrincipalId::parse("member").unwrap()),
            from: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            to: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        }
    }

    fn position() -> DailyUsagePosition {
        DailyUsagePosition {
            day: NaiveDate::from_ymd_opt(2026, 10, 13).unwrap(),
            principal: PrincipalId::parse("member").unwrap(),
            server: ServerSlug::parse("media").unwrap(),
            kind: UsageKind::parse("media.generation").unwrap(),
            phase: UsagePhase::Actual,
            currency: Some(Currency::parse("USD").unwrap()),
        }
    }

    #[test]
    fn a_cursor_continues_only_its_own_view() {
        let own = codec(UsageScope::Own);
        let encoded = own.encode(&position()).unwrap();
        assert_eq!(own.decode(&encoded).unwrap(), position());
        assert!(codec(UsageScope::Tenant).decode(&encoded).is_err());
        let mut later = codec(UsageScope::Own);
        later.from = NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
        assert!(later.decode(&encoded).is_err());
        let mut other = codec(UsageScope::Own);
        other.principal = Some(PrincipalId::parse("admin").unwrap());
        assert!(other.decode(&encoded).is_err());
        assert!(own.decode("not a cursor").is_err());
        assert!(own.decode(&"a".repeat(MAX_CURSOR_BYTES + 1)).is_err());
    }
}
