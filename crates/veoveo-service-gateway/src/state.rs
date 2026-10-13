use anyhow::{Context, Result};
use veoveo_platform_store::{PlatformStore, StoreConfig};

use crate::store::GatewayStore;

mod auth_state;
mod refresh_tokens;
mod session;
mod task_routes;

pub use auth_state::GatewayReplayRetentionSummary;
pub use refresh_tokens::{
    GatewayRefreshDeliveryWindow, GatewayRefreshExchange, GatewayRefreshIssueRequest,
    GatewayRefreshRotationRequest, IssuedGatewayRefreshToken, REFRESH_TOKEN_TTL_SECONDS,
    RefreshTokenDeliveryCipher,
};
pub(crate) use task_routes::{GatewayTaskOwnership, GatewayTaskRouteDraft};

/// Shared, installation-wide gateway correctness state.
///
/// Clones may be used by independent gateway replicas; PostgreSQL is the sole authority
/// for replay, OAuth, revocation and audit data.
#[derive(Debug, Clone)]
pub struct GatewayState {
    pub(crate) token_extensions: Option<veoveo_types::ExtensionRegistry>,
    pub(super) store: GatewayStore,
    pub(crate) audit: veoveo_audit::AuditStore,
    pub(crate) audit_writer: std::sync::Arc<tokio::sync::OnceCell<veoveo_audit::AuditWriter>>,
    pub(crate) audit_health: std::sync::Arc<std::sync::OnceLock<veoveo_audit::AuditHealth>>,
    pub(crate) oauth_client_resolver:
        Option<std::sync::Arc<dyn crate::oauth_clients::OAuthClientResolver>>,
}

impl GatewayState {
    pub fn new(platform: PlatformStore) -> Self {
        Self {
            audit: veoveo_audit::AuditStore::new(platform.clone(), crate::audit_registry()),
            store: GatewayStore::new(platform),
            audit_writer: Default::default(),
            audit_health: Default::default(),
            oauth_client_resolver: None,
            token_extensions: None,
        }
    }

    pub async fn connect(config: StoreConfig) -> Result<Self> {
        let platform = PlatformStore::connect(config)
            .await
            .context("failed to connect gateway runtime state to PostgreSQL")?;
        Ok(Self::new(platform))
    }

    pub fn platform_store(&self) -> &PlatformStore {
        self.store.platform()
    }

    /// The Gateway's statements over the installation database.
    pub fn gateway_store(&self) -> &GatewayStore {
        &self.store
    }

    /// The audit log, read and written with the installation's target registry.
    pub fn audit_store(&self) -> &veoveo_audit::AuditStore {
        &self.audit
    }

    /// Admit one audit draft for a write that commits with other state.
    pub(crate) fn audit_write(
        &self,
        draft: &veoveo_audit_contract::AuditDraft,
    ) -> Result<crate::audit::AuditAppend> {
        Ok(crate::audit::AuditAppend::new(
            veoveo_audit_contract::AuditTransactionWrite::new(self.audit.registry(), draft.clone())
                .context("audit draft is outside the installation's audit registry")?,
        ))
    }
}
