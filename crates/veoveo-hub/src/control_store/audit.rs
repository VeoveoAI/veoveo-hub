//! The activated revision and its publication record share one commit.
use crate::audit::hub_audit;
use anyhow::Result;
use veoveo_audit_contract::{AuditContext, AuditOutcome, AuditRegistry, AuditTransactionWrite};

pub(super) fn changes(
    registry: &AuditRegistry,
    context: &AuditContext,
    revision: &crate::control_plane::HubControlPlaneRevision,
) -> Result<AuditTransactionWrite> {
    let record = context
        .draft(
            hub_audit().detail(&crate::audit::ControlPlanePublication {
                revision: revision.revision_id.clone(),
                digest: veoveo_types::Sha256Digest::parse(format!("sha256:{}", revision.sha256))?,
            })?,
            AuditOutcome::Success,
        )
        .target(crate::audit::hub_target()?)
        .build()?;
    Ok(AuditTransactionWrite::batch(registry, vec![record])?)
}
