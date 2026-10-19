//! The activated revision and each changed Work Context share one commit.
use crate::audit::{AccountActivity, AccountChange, hub_audit, work_context_target};
use crate::control_plane::WorkContextDefinition;
use anyhow::Result;
use std::collections::BTreeMap;
use veoveo_audit_contract::{AuditContext, AuditOutcome, AuditRegistry, AuditTransactionWrite};

pub(super) fn changes(
    registry: &AuditRegistry,
    before: &[WorkContextDefinition],
    after: &[WorkContextDefinition],
    context: &AuditContext,
    revision: &crate::control_plane::HubControlPlaneRevision,
) -> Result<AuditTransactionWrite> {
    let before = before
        .iter()
        .map(|value| ((&value.tenant, &value.id), value))
        .collect::<BTreeMap<_, _>>();
    let after = after
        .iter()
        .map(|value| ((&value.tenant, &value.id), value))
        .collect::<BTreeMap<_, _>>();
    let mut records = vec![
        context
            .draft(
                hub_audit().detail(&crate::audit::ControlPlanePublication {
                    revision: revision.revision_id.clone(),
                    digest: veoveo_types::Sha256Digest::parse(format!(
                        "sha256:{}",
                        revision.sha256
                    ))?,
                })?,
                AuditOutcome::Success,
            )
            .target(crate::audit::hub_target()?)
            .build()?,
    ];
    for (key, value) in &after {
        let activity = match before.get(key) {
            None => AccountActivity::Create,
            Some(old) if *old == *value => continue,
            Some(_) => AccountActivity::Update,
        };
        records.push(
            context
                .draft(
                    hub_audit().detail(&AccountChange { activity })?,
                    AuditOutcome::Success,
                )
                .target(work_context_target(&value.tenant, &value.id)?)
                .build()?,
        );
    }
    for (key, value) in &before {
        if !after.contains_key(key) {
            records.push(
                context
                    .draft(
                        hub_audit().detail(&AccountChange {
                            activity: AccountActivity::Delete,
                        })?,
                        AuditOutcome::Success,
                    )
                    .target(work_context_target(&value.tenant, &value.id)?)
                    .build()?,
            );
        }
    }
    Ok(AuditTransactionWrite::batch(registry, records)?)
}
