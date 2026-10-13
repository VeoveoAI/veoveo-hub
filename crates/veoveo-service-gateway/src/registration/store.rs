//! Registrations, namespace claims and registration logins in the `gateway` schema.
//! Every replica reads the same rows, so a registration admitted by one replica routes
//! on all of them.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use sqlx::types::Json;
use veoveo_platform_store::{LoginName, PlatformStore};
use veoveo_sdk_rust::registration::{ClientMetadata, SoftwareStatement};
use veoveo_types::{OAuthClientId, ServerSlug, Sha256Digest};

/// One registration as the service reads it back.
#[derive(Clone, Debug)]
pub(crate) struct StoredRegistration {
    pub client_id: OAuthClientId,
    pub metadata: ClientMetadata,
    pub software_statement: SoftwareStatement,
    pub token_digest: Sha256Digest,
    pub issued_at: DateTime<Utc>,
}

struct RegistrationRow {
    client_id: String,
    server: String,
    metadata: Json<ClientMetadata>,
    software_statement: String,
    access_token_digest: String,
    issued_at: DateTime<Utc>,
}

impl TryFrom<RegistrationRow> for StoredRegistration {
    type Error = anyhow::Error;

    fn try_from(row: RegistrationRow) -> Result<Self> {
        let metadata = row.metadata.0;
        anyhow::ensure!(
            metadata.slug().as_str() == row.server,
            "stored registration server differs from its metadata"
        );
        Ok(Self {
            client_id: OAuthClientId::parse(row.client_id)
                .context("stored registration has an invalid client identifier")?,
            metadata,
            software_statement: SoftwareStatement::new(row.software_statement)
                .context("stored software statement is malformed")?,
            token_digest: Sha256Digest::parse(row.access_token_digest)
                .context("stored registration access token digest is malformed")?,
            issued_at: row.issued_at,
        })
    }
}

/// What registering one server writes.
pub(crate) struct NewRegistration<'a> {
    pub client_id: &'a OAuthClientId,
    pub metadata: &'a ClientMetadata,
    pub software_statement: &'a SoftwareStatement,
    pub token_digest: &'a Sha256Digest,
    pub login: &'a LoginName,
    pub password: &'a SecretString,
    pub now: DateTime<Utc>,
}

/// How a registration related to the server's namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pin {
    /// This registration claimed the namespace.
    Claimed,
    /// The namespace was already pinned to the same key.
    Held,
    /// Another key holds the namespace.
    Foreign,
}

/// A registration that ended, and the login it held.
#[derive(Clone, Debug)]
pub(crate) struct EndedRegistration {
    pub client_id: OAuthClientId,
    pub server: ServerSlug,
    pub login: LoginName,
}

struct EndedRow {
    client_id: String,
    server: String,
    login: String,
}

impl TryFrom<EndedRow> for EndedRegistration {
    type Error = anyhow::Error;

    fn try_from(row: EndedRow) -> Result<Self> {
        Ok(Self {
            client_id: OAuthClientId::parse(row.client_id)?,
            server: ServerSlug::parse(row.server)?,
            login: LoginName::parse(row.login)?,
        })
    }
}

/// Whether a registered server's login holds a live kernel instance, and since when
/// housekeeping has found it without one.
#[derive(Clone, Debug)]
pub(crate) struct Liveness {
    pub server: ServerSlug,
    pub orphaned_at: Option<DateTime<Utc>>,
    pub alive: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct RegistrationStore {
    store: PlatformStore,
}

impl RegistrationStore {
    pub fn new(store: PlatformStore) -> Self {
        Self { store }
    }

    pub fn platform(&self) -> &PlatformStore {
        &self.store
    }

    /// The RFC 7638 thumbprint the server's namespace is pinned to.
    pub async fn claim(&self, server: &ServerSlug) -> Result<Option<String>> {
        sqlx::query_file_scalar!("src/registration/store/claim.sql", server.as_str())
            .fetch_optional(self.store.pool())
            .await
            .context("failed to read the namespace claim")
    }

    /// Pin the namespace on the server's first registration, create the server's group
    /// role and schema, the registration's login and its row, in one transaction.
    pub async fn register(&self, registration: NewRegistration<'_>) -> Result<Pin> {
        let server = registration.metadata.slug();
        let thumbprint = registration.metadata.key().thumbprint();
        let mut transaction = self.store.pool().begin().await?;
        let claimed = sqlx::query_file_scalar!(
            "src/registration/store/create_claim.sql",
            server.as_str(),
            thumbprint.as_str(),
            registration.now,
        )
        .fetch_optional(&mut *transaction)
        .await
        .context("failed to claim the namespace")?;
        let pin = if claimed.is_some() {
            Pin::Claimed
        } else {
            let pinned =
                sqlx::query_file_scalar!("src/registration/store/claim.sql", server.as_str())
                    .fetch_one(&mut *transaction)
                    .await
                    .context("failed to read the namespace claim")?;
            if pinned != thumbprint.as_str() {
                return Ok(Pin::Foreign);
            }
            Pin::Held
        };
        sqlx::query_file!("src/registration/store/ensure_server.sql", server.as_str())
            .execute(&mut *transaction)
            .await
            .context("failed to create the server's role and schema")?;
        sqlx::query_file!(
            "src/registration/store/create_login.sql",
            server.as_str(),
            registration.login.as_str(),
            registration.password.expose_secret(),
        )
        .execute(&mut *transaction)
        .await
        .context("failed to create the registration's login")?;
        sqlx::query_file!(
            "src/registration/store/create_registration.sql",
            registration.client_id.as_str(),
            server.as_str(),
            Json(registration.metadata) as _,
            registration.software_statement.as_str(),
            registration.token_digest.as_str(),
            registration.login.as_str(),
            registration.now,
        )
        .execute(&mut *transaction)
        .await
        .context("failed to record the registration")?;
        transaction.commit().await?;
        Ok(pin)
    }

    pub async fn registrations(&self) -> Result<Vec<StoredRegistration>> {
        sqlx::query_file_as!(RegistrationRow, "src/registration/store/registrations.sql")
            .fetch_all(self.store.pool())
            .await
            .context("failed to read the registrations")?
            .into_iter()
            .map(StoredRegistration::try_from)
            .collect()
    }

    pub async fn registration(
        &self,
        client_id: &OAuthClientId,
    ) -> Result<Option<StoredRegistration>> {
        sqlx::query_file_as!(
            RegistrationRow,
            "src/registration/store/registration.sql",
            client_id.as_str(),
        )
        .fetch_optional(self.store.pool())
        .await
        .context("failed to read the registration")?
        .map(StoredRegistration::try_from)
        .transpose()
    }

    /// Replace a registration's metadata. Returns false when the registration ended.
    pub async fn update(
        &self,
        client_id: &OAuthClientId,
        metadata: &ClientMetadata,
        software_statement: &SoftwareStatement,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let updated = sqlx::query_file_scalar!(
            "src/registration/store/update_registration.sql",
            client_id.as_str(),
            Json(metadata) as _,
            software_statement.as_str(),
            now,
        )
        .fetch_optional(self.store.pool())
        .await
        .context("failed to update the registration")?;
        Ok(updated.is_some())
    }

    /// Remove one registration and drop its login, in one transaction.
    pub async fn deregister(&self, client_id: &OAuthClientId) -> Result<Option<EndedRegistration>> {
        let mut transaction = self.store.pool().begin().await?;
        let Some(ended) = sqlx::query_file_as!(
            EndedRow,
            "src/registration/store/delete_registration.sql",
            client_id.as_str(),
        )
        .fetch_optional(&mut *transaction)
        .await
        .context("failed to remove the registration")?
        else {
            return Ok(None);
        };
        let ended = EndedRegistration::try_from(ended)?;
        drop_login(&mut transaction, &ended.login).await?;
        transaction.commit().await?;
        Ok(Some(ended))
    }

    /// Record the registrations whose logins newly hold no live kernel instance as
    /// orphaned at `now` and clear the mark of those that hold one again, then remove
    /// the registrations orphaned before `stale_before`, with their logins. A
    /// registration whose liveness did not change is not written.
    pub async fn collect(
        &self,
        now: DateTime<Utc>,
        stale_before: DateTime<Utc>,
    ) -> Result<Vec<EndedRegistration>> {
        sqlx::query_file!("src/registration/store/clear_orphaned.sql")
            .execute(self.store.pool())
            .await
            .context("failed to clear revived registrations")?;
        sqlx::query_file!("src/registration/store/mark_orphaned.sql", now)
            .execute(self.store.pool())
            .await
            .context("failed to mark orphaned registrations")?;
        let mut transaction = self.store.pool().begin().await?;
        let ended =
            sqlx::query_file_as!(EndedRow, "src/registration/store/collect.sql", stale_before,)
                .fetch_all(&mut *transaction)
                .await
                .context("failed to remove abandoned registrations")?
                .into_iter()
                .map(EndedRegistration::try_from)
                .collect::<Result<Vec<_>>>()?;
        for registration in &ended {
            drop_login(&mut transaction, &registration.login).await?;
        }
        transaction.commit().await?;
        Ok(ended)
    }

    /// The liveness of these registrations.
    pub async fn liveness(&self, clients: &[OAuthClientId]) -> Result<Vec<Liveness>> {
        let clients = clients.iter().map(ToString::to_string).collect::<Vec<_>>();
        sqlx::query_file!("src/registration/store/liveness.sql", &clients)
            .fetch_all(self.store.pool())
            .await
            .context("failed to read registration liveness")?
            .into_iter()
            .map(|row| {
                Ok(Liveness {
                    server: ServerSlug::parse(row.server)?,
                    orphaned_at: row.orphaned_at,
                    alive: row.alive,
                })
            })
            .collect()
    }
}

async fn drop_login(connection: &mut sqlx::PgConnection, login: &LoginName) -> Result<()> {
    sqlx::query_file!("src/registration/store/drop_login.sql", login.as_str())
        .execute(&mut *connection)
        .await
        .context("failed to drop the registration's login")?;
    Ok(())
}
