//! The Hub's database state: hosted OAuth requests and codes, refresh-token
//! families, JWT revocations, replay identifiers, published control-plane revisions with
//! the identity rows they define, principals' selections, and the cleanup of expired Task
//! routes. The Hub
//! owns the `hub` schema and applies its migrations after core's kernel migrations.
//! Registrations and Task routes run their own statements against the same schema.

mod authorization;
mod device;
mod execution;
pub use device::{DeviceApproval, DeviceAuthorization, DevicePoll, NewDeviceAuthorization};
mod computers;
pub use computers::{Computer, ComputerHandoff};
mod control;
mod refresh;
mod replay;
mod revocations;
mod selection;
mod task_routes;

pub use control::{ControlActivation, ControlObject, ControlPublication};
pub use refresh::{
    NewRefreshFamily, NewRefreshToken, RefreshBinding, RefreshDeliveryEnvelope, RefreshFamily,
    RefreshPrincipal, RefreshRetention, RefreshRevocation, RefreshRotation, RefreshToken,
};
pub use replay::{ReplayIdentifier, ReplayKind};

use std::future::Future;

use sqlx::{PgConnection, PgPool, migrate::Migrator};
use veoveo_store::{Channel, PlatformStore, StoreError};

/// The Hub's schema. No registered server may take it as its slug.
pub const SCHEMA: &str = "hub";

/// The channel on which `hub` schema triggers publish changes, such as registrations.
pub fn channel() -> Channel {
    Channel::parse(SCHEMA).expect("the hub schema name is a channel")
}

/// The `hub` schema migrations. Their history lives in `hub._sqlx_migrations`,
/// as `sqlx.toml` declares.
pub fn migrator() -> Migrator {
    sqlx::migrate!()
}

/// The Hub's statements over core's connection pool.
#[derive(Clone, Debug)]
pub struct HubStore {
    platform: PlatformStore,
}

impl HubStore {
    pub fn new(platform: PlatformStore) -> Self {
        Self { platform }
    }

    pub fn platform(&self) -> &PlatformStore {
        &self.platform
    }

    fn pool(&self) -> &PgPool {
        self.platform.pool()
    }

    /// Apply core's kernel migrations, then the Hub's own.
    pub async fn migrate(&self) -> Result<(), HubStoreError> {
        self.platform.migrate().await?;
        migrator().run(self.pool()).await?;
        Ok(())
    }
}

/// A failed Hub statement.
#[derive(Debug, thiserror::Error)]
pub enum HubStoreError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("hub migration failed: {0}")]
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

/// Rows another owner records with a hub change, such as the change's audit
/// records. The store appends them on the change's own connection before it commits,
/// and an error rolls the change back.
pub trait TransactionAppend: Sync {
    fn append(
        &self,
        connection: &mut PgConnection,
    ) -> impl Future<Output = Result<(), HubStoreError>> + Send;
}

/// Admit a stored column through its owning type. A row that no longer admits is an
/// error, never a default.
fn admitted<T, E>(column: &'static str, value: Result<T, E>) -> Result<T, HubStoreError> {
    value.map_err(|_| HubStoreError::InvalidStoredValue { column })
}
