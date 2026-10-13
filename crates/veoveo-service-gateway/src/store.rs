//! The Gateway's database state: hosted OAuth requests and codes, refresh-token
//! families, JWT revocations, replay identifiers, published control-plane revisions with
//! the identity rows they define, and the cleanup of expired Task routes. The Gateway
//! owns the `gateway` schema and applies its migrations after core's kernel migrations.
//! Registrations and Task routes run their own statements against the same schema.

mod authorization;
mod control;
mod refresh;
mod replay;
mod revocations;
mod task_routes;

pub use control::{ControlObject, ControlPublication};
pub use refresh::{
    NewRefreshFamily, NewRefreshToken, RefreshBinding, RefreshDeliveryEnvelope, RefreshFamily,
    RefreshPrincipal, RefreshRetention, RefreshRevocation, RefreshRotation, RefreshToken,
};
pub use replay::{ReplayIdentifier, ReplayKind};

use std::future::Future;

use sqlx::{PgConnection, PgPool, migrate::Migrator};
use veoveo_platform_store::{Channel, PlatformStore, StoreError};

/// The Gateway's schema. No registered server may take it as its slug.
pub const SCHEMA: &str = "gateway";

/// The channel on which `gateway` schema triggers publish changes, such as registrations.
pub fn channel() -> Channel {
    Channel::parse(SCHEMA).expect("the gateway schema name is a channel")
}

/// The `gateway` schema migrations. Their history lives in `gateway._sqlx_migrations`,
/// as `sqlx.toml` declares.
pub static MIGRATOR: Migrator = sqlx::migrate!();

/// The Gateway's statements over core's connection pool.
#[derive(Clone, Debug)]
pub struct GatewayStore {
    platform: PlatformStore,
}

impl GatewayStore {
    pub fn new(platform: PlatformStore) -> Self {
        Self { platform }
    }

    pub fn platform(&self) -> &PlatformStore {
        &self.platform
    }

    fn pool(&self) -> &PgPool {
        self.platform.pool()
    }

    /// Apply core's kernel migrations, then the Gateway's own.
    pub async fn migrate(&self) -> Result<(), GatewayStoreError> {
        self.platform.migrate().await?;
        MIGRATOR.run(self.pool()).await?;
        Ok(())
    }
}

/// A failed Gateway statement.
#[derive(Debug, thiserror::Error)]
pub enum GatewayStoreError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("gateway migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("the stored {column} is not an admitted value")]
    InvalidStoredValue { column: &'static str },
    #[error("invalid refresh-token transition: {reason}")]
    InvalidRefreshTransition { reason: &'static str },
    #[error("another control-plane revision became active first")]
    ControlRevisionChanged,
    #[error("the control-plane revision already exists")]
    ControlRevisionExists,
    #[error("records appended with the change failed: {0}")]
    Append(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Rows another owner records with a gateway change, such as the change's audit
/// records. The store appends them on the change's own connection before it commits,
/// and an error rolls the change back.
pub trait TransactionAppend: Sync {
    fn append(
        &self,
        connection: &mut PgConnection,
    ) -> impl Future<Output = Result<(), GatewayStoreError>> + Send;
}

/// Admit a stored column through its owning type. A row that no longer admits is an
/// error, never a default.
fn admitted<T, E>(column: &'static str, value: Result<T, E>) -> Result<T, GatewayStoreError> {
    value.map_err(|_| GatewayStoreError::InvalidStoredValue { column })
}
