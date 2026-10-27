//! Registrations, namespace claims, instances and registration logins in the `hub`
//! schema. Every replica reads the same rows, so a registration admitted by one replica
//! routes on all of them.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use sqlx::types::Json;
use veoveo_hub_sdk::registration::StorageBinding;
use veoveo_hub_sdk::registration::{ClientMetadata, SoftwareStatement};
use veoveo_store::{LoginName, PlatformStore};
use veoveo_types::{DeploymentId, OAuthClientId, ServerSlug, Sha256Digest};

use crate::data_environment::{self, DatabaseName};
use crate::execution::BranchName;

/// One registration as the service reads it back.
#[derive(Clone, Debug)]
pub(crate) struct StoredRegistration {
    pub client_id: OAuthClientId,
    pub metadata: ClientMetadata,
    pub software_statement: SoftwareStatement,
    pub token_digest: Sha256Digest,
    pub issued_at: DateTime<Utc>,
    /// When housekeeping found its login without a live kernel instance, until it
    /// holds one again.
    pub orphaned_at: Option<DateTime<Utc>>,
    /// The branch of the instance the registration's deployment is, if it is one.
    pub branch: Option<BranchName>,
}

struct RegistrationRow {
    client_id: String,
    server: String,
    metadata: Json<ClientMetadata>,
    software_statement: String,
    access_token_digest: String,
    issued_at: DateTime<Utc>,
    orphaned_at: Option<DateTime<Utc>>,
    branch: Option<String>,
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
            orphaned_at: row.orphaned_at,
            branch: row
                .branch
                .map(BranchName::parse)
                .transpose()
                .context("stored instance branch is malformed")?,
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
    /// Another key holds the namespace, or the namespace of the deployment's
    /// registered server.
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
    pub deployment: DeploymentId,
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
    /// role and schema, the registration's login and its row, in one transaction. A
    /// deployment registered before with other metadata takes the new metadata and
    /// storage binding, and its previous registrations end with their logins; when it
    /// belonged to another slug, that slug's namespace must be pinned to the same key.
    /// Returns the replaced registrations.
    pub async fn register(
        &self,
        registration: NewRegistration<'_>,
    ) -> Result<(Pin, Vec<EndedRegistration>)> {
        let server = registration.metadata.slug();
        let thumbprint = registration.metadata.key().thumbprint();
        let mut transaction = self.store.pool().begin().await?;
        let binding = StorageBinding::SharedLogicalSchema {
            server: server.clone(),
        };
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
                    .context("failed to read namespace claim")?;
            if pinned != thumbprint.as_str() {
                return Ok((Pin::Foreign, Vec::new()));
            }
            Pin::Held
        };
        let deployment = registration.metadata.registration.deployment.as_uuid();
        sqlx::query_file!(
            "src/registration/store/create_deployment.sql",
            deployment,
            server.as_str(),
            Json(registration.metadata) as _,
            Json(&binding) as _,
            registration.now,
        )
        .execute(&mut *transaction)
        .await?;
        let stored = sqlx::query_file!("src/registration/store/deployment.sql", deployment)
            .fetch_one(&mut *transaction)
            .await?;
        let mut replaced = Vec::new();
        if stored.metadata.0 != *registration.metadata || stored.storage.0 != binding {
            if stored.server != server.as_str() {
                let pinned =
                    sqlx::query_file_scalar!("src/registration/store/claim.sql", stored.server)
                        .fetch_optional(&mut *transaction)
                        .await
                        .context("failed to read the deployment's namespace claim")?;
                if pinned.as_deref() != Some(thumbprint.as_str()) {
                    return Ok((Pin::Foreign, Vec::new()));
                }
            }
            replaced = replace(&mut transaction, &registration, &binding).await?;
        }
        admit(&mut transaction, &registration).await?;
        transaction.commit().await?;
        Ok((pin, replaced))
    }

    /// Record a new instance of `branch` of `repository` in `database` under the digest
    /// of its registration token.
    pub async fn create_instance(
        &self,
        deployment: DeploymentId,
        token_digest: &Sha256Digest,
        repository: &str,
        database: &DatabaseName,
        branch: &BranchName,
        now: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query_file!(
            "src/registration/store/create_instance.sql",
            deployment.as_uuid(),
            token_digest.as_str(),
            repository,
            database.as_str(),
            branch.as_str(),
            now,
        )
        .execute(self.store.pool())
        .await
        .context("failed to record the instance")?;
        Ok(())
    }

    /// Replace the digest of the instance's registration token.
    pub async fn reissue_token(
        &self,
        deployment: DeploymentId,
        token_digest: &Sha256Digest,
    ) -> Result<()> {
        let replaced = sqlx::query_file!(
            "src/registration/store/reissue_token.sql",
            deployment.as_uuid(),
            token_digest.as_str(),
        )
        .execute(self.store.pool())
        .await
        .context("failed to replace the instance's registration token")?;
        anyhow::ensure!(
            replaced.rows_affected() == 1,
            "no instance has deployment {deployment}"
        );
        Ok(())
    }

    /// Record that the server of `deployment`, when it is an instance's, registered or
    /// resumed its registration just now.
    /// Whether a registered instance of another repository than `deployment`'s instance
    /// serves `server`.
    pub async fn served_elsewhere(
        &self,
        server: &ServerSlug,
        deployment: DeploymentId,
    ) -> Result<bool> {
        sqlx::query_file_scalar!(
            "src/registration/store/served_elsewhere.sql",
            server.as_str(),
            deployment.as_uuid()
        )
        .fetch_one(self.store.pool())
        .await
        .context("failed to read which repositories serve the slug")
    }

    pub async fn served(&self, deployment: DeploymentId) -> Result<()> {
        sqlx::query_file!("src/registration/store/served.sql", deployment.as_uuid())
            .execute(self.store.pool())
            .await
            .context("failed to record the instance's server")?;
        Ok(())
    }

    /// The deployment and database of the instance whose registration token has this
    /// digest.
    pub async fn instance(
        &self,
        token_digest: &Sha256Digest,
    ) -> Result<Option<(DeploymentId, DatabaseName)>> {
        sqlx::query_file!("src/registration/store/instance.sql", token_digest.as_str())
            .fetch_optional(self.store.pool())
            .await
            .context("failed to read the instance")?
            .map(|row| {
                Ok((
                    DeploymentId::try_from(row.deployment)
                        .context("stored instance deployment is invalid")?,
                    DatabaseName::parse(row.database)
                        .context("stored instance database is invalid")?,
                ))
            })
            .transpose()
    }

    /// Replace the registration of an instance, in one transaction: write the
    /// deployment's server, metadata and storage binding, remove the deployment's
    /// previous registrations and drop their logins, then create the server's group
    /// role and schema, the new login and its row. An instance in a data environment
    /// first gets the server's group role and schema in that database, on a connection
    /// of its own. Returns the replaced registrations.
    pub async fn register_instance(
        &self,
        registration: NewRegistration<'_>,
        environment: Option<&DatabaseName>,
    ) -> Result<Vec<EndedRegistration>> {
        let server = registration.metadata.slug();
        let binding = StorageBinding::SharedLogicalSchema {
            server: server.clone(),
        };
        if let Some(environment) = environment {
            let store = data_environment::connect(&self.store, environment).await?;
            let ensured =
                sqlx::query_file!("src/registration/store/ensure_server.sql", server.as_str())
                    .execute(store.pool())
                    .await;
            store.close().await;
            ensured.with_context(|| {
                format!("failed to create the server's role and schema in {environment}")
            })?;
        }
        let mut transaction = self.store.pool().begin().await?;
        let replaced = replace(&mut transaction, &registration, &binding).await?;
        admit(&mut transaction, &registration).await?;
        transaction.commit().await?;
        Ok(replaced)
    }

    /// Remove an instance and its registrations, dropping their logins, in one
    /// transaction. Returns the instance's database and the removed registrations, or
    /// none when no instance has the deployment.
    pub async fn remove_instance(
        &self,
        deployment: DeploymentId,
    ) -> Result<Option<(DatabaseName, Vec<EndedRegistration>)>> {
        let mut transaction = self.store.pool().begin().await?;
        let ended = sqlx::query_file_as!(
            EndedRow,
            "src/registration/store/replace_registrations.sql",
            deployment.as_uuid(),
        )
        .fetch_all(&mut *transaction)
        .await
        .context("failed to remove the instance's registrations")?
        .into_iter()
        .map(EndedRegistration::try_from)
        .collect::<Result<Vec<_>>>()?;
        for registration in &ended {
            drop_login(&mut transaction, &registration.login).await?;
        }
        let Some(database) = sqlx::query_file_scalar!(
            "src/registration/store/delete_instance.sql",
            deployment.as_uuid()
        )
        .fetch_optional(&mut *transaction)
        .await
        .context("failed to remove the instance")?
        else {
            return Ok(None);
        };
        transaction.commit().await?;
        Ok(Some((
            DatabaseName::parse(database).context("stored instance database is invalid")?,
            ended,
        )))
    }

    /// Whether `deployment` is an instance's.
    pub async fn is_instance(&self, deployment: DeploymentId) -> Result<bool> {
        sqlx::query_file_scalar!(
            "src/registration/store/deployment_instance.sql",
            deployment.as_uuid()
        )
        .fetch_one(self.store.pool())
        .await
        .context("failed to look up the deployment's instance")
    }

    /// Replace a registration's metadata and software statement, and its deployment's
    /// metadata, in one transaction.
    pub async fn update(
        &self,
        client_id: &OAuthClientId,
        metadata: &ClientMetadata,
        software_statement: &SoftwareStatement,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let mut transaction = self.store.pool().begin().await?;
        sqlx::query_file!(
            "src/registration/store/update_registration.sql",
            client_id.as_str(),
            Json(metadata) as _,
            software_statement.as_str(),
            now,
        )
        .execute(&mut *transaction)
        .await
        .context("failed to update the registration")?;
        sqlx::query_file!(
            "src/registration/store/update_deployment.sql",
            metadata.registration.deployment.as_uuid(),
            Json(metadata) as _,
        )
        .execute(&mut *transaction)
        .await
        .context("failed to update the registration's deployment")?;
        transaction.commit().await?;
        Ok(())
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
                    deployment: DeploymentId::parse(row.deployment.to_string())?,
                    orphaned_at: row.orphaned_at,
                    alive: row.alive,
                })
            })
            .collect()
    }
}

/// Write the deployment's server, metadata and storage binding, remove its previous
/// registrations and drop their logins. Returns the removed registrations.
async fn replace(
    connection: &mut sqlx::PgConnection,
    registration: &NewRegistration<'_>,
    binding: &StorageBinding,
) -> Result<Vec<EndedRegistration>> {
    let deployment = registration.metadata.registration.deployment.as_uuid();
    sqlx::query_file!(
        "src/registration/store/replace_deployment.sql",
        deployment,
        registration.metadata.slug().as_str(),
        Json(registration.metadata) as _,
        Json(binding) as _,
        registration.now,
    )
    .execute(&mut *connection)
    .await
    .context("failed to record the deployment")?;
    let replaced = sqlx::query_file_as!(
        EndedRow,
        "src/registration/store/replace_registrations.sql",
        deployment,
    )
    .fetch_all(&mut *connection)
    .await
    .context("failed to remove the deployment's previous registrations")?
    .into_iter()
    .map(EndedRegistration::try_from)
    .collect::<Result<Vec<_>>>()?;
    for previous in &replaced {
        drop_login(&mut *connection, &previous.login).await?;
    }
    Ok(replaced)
}

/// Create the server's group role and schema, the registration's login and its row.
async fn admit(
    connection: &mut sqlx::PgConnection,
    registration: &NewRegistration<'_>,
) -> Result<()> {
    let server = registration.metadata.slug();
    sqlx::query_file!("src/registration/store/ensure_server.sql", server.as_str())
        .execute(&mut *connection)
        .await
        .context("failed to create the server's role and schema")?;
    sqlx::query_file!(
        "src/registration/store/create_login.sql",
        server.as_str(),
        registration.login.as_str(),
        registration.password.expose_secret(),
    )
    .execute(&mut *connection)
    .await
    .context("failed to create the registration's login")?;
    sqlx::query_file!(
        "src/registration/store/create_registration.sql",
        registration.client_id.as_str(),
        registration.metadata.registration.deployment.as_uuid(),
        server.as_str(),
        Json(registration.metadata) as _,
        registration.software_statement.as_str(),
        registration.token_digest.as_str(),
        registration.login.as_str(),
        registration.now,
    )
    .execute(&mut *connection)
    .await
    .context("failed to record the registration")?;
    Ok(())
}

async fn drop_login(connection: &mut sqlx::PgConnection, login: &LoginName) -> Result<()> {
    sqlx::query_file!("src/registration/store/drop_login.sql", login.as_str())
        .execute(&mut *connection)
        .await
        .context("failed to drop the registration's login")?;
    Ok(())
}
