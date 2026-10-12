//! Registrations and namespace claims in the platform store. Every replica reads the
//! same rows, so a registration admitted by one replica routes on all of them.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};
use surrealdb::types::{Error, Kind, RecordId, SurrealValue, Value};
use veoveo_mcp_ext_registration::{ClientMetadata, DatabaseIdentifier, SoftwareStatement};
use veoveo_modules::ModuleManifest;
use veoveo_platform_store::PlatformStore;
use veoveo_types::{OAuthClientId, ServerSlug};

pub(crate) const REGISTRATION_TABLE: &str = "gateway_registration";
pub(crate) const CLAIM_TABLE: &str = "gateway_namespace_claim";

/// A typed document stored as one flexible object.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Document<T>(pub T);

impl<T: Serialize + DeserializeOwned> SurrealValue for Document<T> {
    fn kind_of() -> Kind {
        Kind::Object
    }

    fn into_value(self) -> Value {
        veoveo_platform_store::native_json_into_value(
            serde_json::to_value(self.0).expect("typed registration document"),
        )
    }

    fn from_value(value: Value) -> Result<Self, Error> {
        serde_json::from_value(veoveo_platform_store::native_json_from_value_strict(value)?)
            .map(Self)
            .map_err(|_| Error::internal("invalid registration document".into()))
    }
}

#[derive(Clone, Debug, SurrealValue)]
pub(crate) struct RegistrationRecord {
    pub id: RecordId,
    pub slug: String,
    pub metadata: Document<ClientMetadata>,
    pub software_statement: String,
    pub registration_access_token_digest: String,
    pub database_username: Option<String>,
    pub issued_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub lease_expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, SurrealValue)]
pub(crate) struct ClaimRecord {
    pub id: RecordId,
    pub key_thumbprint: String,
    pub module: Option<Document<ModuleManifest>>,
    pub claimed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, SurrealValue)]
struct ClaimContent {
    key_thumbprint: String,
    module: Option<Document<ModuleManifest>>,
    claimed_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, SurrealValue)]
struct RegistrationContent {
    slug: String,
    metadata: Document<ClientMetadata>,
    software_statement: String,
    registration_access_token_digest: String,
    database_username: Option<String>,
    issued_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    lease_expires_at: DateTime<Utc>,
}

/// One registration as the service reads it back.
#[derive(Clone, Debug)]
pub(crate) struct StoredRegistration {
    pub client_id: OAuthClientId,
    pub metadata: ClientMetadata,
    pub software_statement: SoftwareStatement,
    pub token_digest: String,
    pub database_username: Option<DatabaseIdentifier>,
    pub issued_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub lease_expires_at: DateTime<Utc>,
}

impl TryFrom<RegistrationRecord> for StoredRegistration {
    type Error = anyhow::Error;

    fn try_from(record: RegistrationRecord) -> Result<Self> {
        let client_id = OAuthClientId::parse(record_key(&record.id)?)
            .context("stored registration has an invalid client identifier")?;
        let metadata = record.metadata.0;
        anyhow::ensure!(
            metadata.slug().as_str() == record.slug,
            "stored registration slug differs from its metadata"
        );
        Ok(Self {
            client_id,
            metadata,
            software_statement: SoftwareStatement::new(record.software_statement)
                .context("stored software statement is malformed")?,
            token_digest: record.registration_access_token_digest,
            database_username: record
                .database_username
                .map(DatabaseIdentifier::new)
                .transpose()
                .context("stored database user name is invalid")?,
            issued_at: record.issued_at,
            updated_at: record.updated_at,
            lease_expires_at: record.lease_expires_at,
        })
    }
}

/// One namespace claim: the key a slug is pinned to and the module history the
/// Gateway has applied for it.
#[derive(Clone, Debug)]
pub(crate) struct NamespaceClaim {
    pub slug: ServerSlug,
    pub key_thumbprint: String,
    pub module: Option<ModuleManifest>,
}

impl TryFrom<ClaimRecord> for NamespaceClaim {
    type Error = anyhow::Error;

    fn try_from(record: ClaimRecord) -> Result<Self> {
        Ok(Self {
            slug: ServerSlug::parse(record_key(&record.id)?)
                .context("stored namespace claim has an invalid slug")?,
            key_thumbprint: record.key_thumbprint,
            module: record.module.map(|module| module.0),
        })
    }
}

fn record_key(id: &RecordId) -> Result<String> {
    match &id.key {
        surrealdb::types::RecordIdKey::String(key) => Ok(key.clone()),
        _ => anyhow::bail!("registration record key must be a string"),
    }
}

pub(crate) fn registration_record(client_id: &OAuthClientId) -> RecordId {
    RecordId::new(REGISTRATION_TABLE, client_id.as_str())
}

fn claim_record(slug: &ServerSlug) -> RecordId {
    RecordId::new(CLAIM_TABLE, slug.as_str())
}

/// What a new or updated registration row holds.
pub(crate) struct RegistrationWrite<'a> {
    pub client_id: &'a OAuthClientId,
    pub metadata: &'a ClientMetadata,
    pub software_statement: &'a SoftwareStatement,
    pub token_digest: &'a str,
    pub database_username: Option<&'a DatabaseIdentifier>,
    pub issued_at: DateTime<Utc>,
    pub now: DateTime<Utc>,
    pub lease_expires_at: DateTime<Utc>,
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

    pub async fn claims(&self) -> Result<Vec<NamespaceClaim>> {
        let records: Vec<ClaimRecord> = self
            .store
            .client()
            .query(include_str!("../queries/registration/claims.surql"))
            .await?
            .check()?
            .take(0)?;
        records.into_iter().map(NamespaceClaim::try_from).collect()
    }

    pub async fn claim(&self, slug: &ServerSlug) -> Result<Option<NamespaceClaim>> {
        let record: Option<ClaimRecord> = self
            .store
            .client()
            .query(include_str!("../queries/registration/claim.surql"))
            .bind(("claim", claim_record(slug)))
            .await?
            .check()?
            .take(0)?;
        record.map(NamespaceClaim::try_from).transpose()
    }

    /// Pin `slug` to the key with `thumbprint`. Returns false when another write
    /// claimed it first; the caller then compares against the stored claim.
    pub async fn create_claim(
        &self,
        slug: &ServerSlug,
        thumbprint: &str,
        module: Option<&ModuleManifest>,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let response = self
            .store
            .client()
            .query(include_str!("../queries/registration/create_claim.surql"))
            .bind(("claim", claim_record(slug)))
            .bind((
                "content",
                ClaimContent {
                    key_thumbprint: thumbprint.to_owned(),
                    module: module.cloned().map(Document),
                    claimed_at: now,
                    updated_at: now,
                },
            ))
            .await?;
        Ok(response.check().is_ok())
    }

    pub async fn record_module(
        &self,
        slug: &ServerSlug,
        module: &ModuleManifest,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.store
            .client()
            .query(include_str!(
                "../queries/registration/record_claim_module.surql"
            ))
            .bind(("claim", claim_record(slug)))
            .bind(("module", Document(module.clone())))
            .bind(("now", now))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn registrations(&self) -> Result<Vec<StoredRegistration>> {
        let records: Vec<RegistrationRecord> = self
            .store
            .client()
            .query(include_str!("../queries/registration/registrations.surql"))
            .await?
            .check()?
            .take(0)?;
        records
            .into_iter()
            .map(StoredRegistration::try_from)
            .collect()
    }

    pub async fn registration(
        &self,
        client_id: &OAuthClientId,
    ) -> Result<Option<StoredRegistration>> {
        let record: Option<RegistrationRecord> = self
            .store
            .client()
            .query(include_str!("../queries/registration/registration.surql"))
            .bind(("registration", registration_record(client_id)))
            .await?
            .check()?
            .take(0)?;
        record.map(StoredRegistration::try_from).transpose()
    }

    pub async fn create_registration(&self, write: RegistrationWrite<'_>) -> Result<()> {
        self.store
            .client()
            .query(include_str!(
                "../queries/registration/create_registration.surql"
            ))
            .bind(("registration", registration_record(write.client_id)))
            .bind(("content", content(&write)))
            .await?
            .check()
            .context("failed to record the registration")?;
        Ok(())
    }

    /// Replace a live registration's metadata and extend its lease. Returns false when
    /// the lease already expired, so an expired registration cannot be revived.
    pub async fn update_registration(&self, write: RegistrationWrite<'_>) -> Result<bool> {
        let updated: Option<RegistrationRecord> = self
            .store
            .client()
            .query(include_str!(
                "../queries/registration/update_registration.surql"
            ))
            .bind(("registration", registration_record(write.client_id)))
            .bind(("content", content(&write)))
            .bind(("now", write.now))
            .await?
            .check()?
            .take(0)?;
        Ok(updated.is_some())
    }

    /// Remove one registration and return what it held.
    pub async fn delete_registration(
        &self,
        client_id: &OAuthClientId,
    ) -> Result<Option<StoredRegistration>> {
        let removed: Option<RegistrationRecord> = self
            .store
            .client()
            .query(include_str!(
                "../queries/registration/delete_registration.surql"
            ))
            .bind(("registration", registration_record(client_id)))
            .await?
            .check()?
            .take(0)?;
        removed.map(StoredRegistration::try_from).transpose()
    }

    /// Remove every registration whose lease ended at or before `now`.
    pub async fn expire(&self, now: DateTime<Utc>) -> Result<Vec<StoredRegistration>> {
        let removed: Vec<RegistrationRecord> = self
            .store
            .client()
            .query(include_str!("../queries/registration/expire.surql"))
            .bind(("now", now))
            .await?
            .check()?
            .take(0)?;
        removed
            .into_iter()
            .map(StoredRegistration::try_from)
            .collect()
    }

    /// Create or replace a database user with editor rights over the whole database.
    pub async fn define_user(&self, username: &DatabaseIdentifier, password: &str) -> Result<()> {
        let literal = serde_json::to_string(password)?;
        self.store
            .client()
            .query(
                include_str!("../queries/registration/define_user.surql")
                    .replace("{password}", &literal),
            )
            .bind(("username", username.as_str().to_owned()))
            .await?
            .check()
            .context("failed to define the server's database user")?;
        Ok(())
    }

    pub async fn remove_user(&self, username: &DatabaseIdentifier) -> Result<()> {
        self.store
            .client()
            .query(include_str!("../queries/registration/remove_user.surql"))
            .bind(("username", username.as_str().to_owned()))
            .await?
            .check()
            .context("failed to remove the server's database user")?;
        Ok(())
    }
}

fn content(write: &RegistrationWrite<'_>) -> RegistrationContent {
    RegistrationContent {
        slug: write.metadata.slug().as_str().to_owned(),
        metadata: Document(write.metadata.clone()),
        software_statement: write.software_statement.as_str().to_owned(),
        registration_access_token_digest: write.token_digest.to_owned(),
        database_username: write
            .database_username
            .map(|username| username.as_str().to_owned()),
        issued_at: write.issued_at,
        updated_at: write.now,
        lease_expires_at: write.lease_expires_at,
    }
}
