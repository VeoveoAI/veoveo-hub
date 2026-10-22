//! Data environments: the PostgreSQL database the servers of one branch name use, in
//! the installation's one PostgreSQL instance. Instances of the base branch use the
//! installation database, which also holds the Hub's own state; every other branch
//! name has a database of its own with core's kernel schemas. Roles belong to the
//! instance, so a server's group role and logins serve every database.
//!
//! The Hub holds no connection to an environment's database: each operation that
//! needs one opens a pool of one connection and closes it.

use std::fmt;

use anyhow::{Context, Result};
use sqlx::AssertSqlSafe;
use veoveo_store::{PlatformStore, StoreConfig, StoreError};
use veoveo_types::DeploymentId;

/// PostgreSQL's identifier limit in bytes.
const MAX_LEN: usize = 63;

/// The name of a data environment's database: 1 to 63 bytes of lowercase ASCII letters,
/// digits and underscores, so it names one database exactly, quoted or not.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DatabaseName(String);

/// A database name outside `[a-z0-9_]{1,63}`.
#[derive(Debug, thiserror::Error)]
#[error("a database name has 1 to 63 lowercase ASCII letters, digits and underscores")]
pub struct InvalidDatabaseName;

impl DatabaseName {
    pub fn parse(value: impl Into<String>) -> Result<Self, InvalidDatabaseName> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= MAX_LEN
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
        if valid {
            Ok(Self(value))
        } else {
            Err(InvalidDatabaseName)
        }
    }

    /// The database `store` connects to: the installation database.
    pub fn installation(store: &PlatformStore) -> Result<Self> {
        Self::parse(store.config().database())
            .context("the installation database name is not a data environment name")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The name as a quoted identifier. The alphabet holds no quote, so quoting
    /// changes nothing but the case rules a bare identifier would follow.
    fn quoted(&self) -> String {
        format!("\"{}\"", self.0)
    }
}

impl fmt::Display for DatabaseName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Whether `name` is the installation database `store` connects to.
pub(crate) fn is_installation(store: &PlatformStore, name: &DatabaseName) -> bool {
    store.config().database() == name.as_str()
}

/// Create the database `name` as the role `store` connects as, which then owns it and
/// needs `CREATEDB`, unless it exists, then apply core's kernel migrations to it.
/// Concurrent callers for one name all succeed: a creation that finds the database
/// created meanwhile counts as done, and the migrator's lock serializes the
/// migrations, which apply once.
pub async fn ensure_database(store: &PlatformStore, name: &DatabaseName) -> Result<()> {
    let environment = match open(store, name).await {
        Ok(environment) => environment,
        Err(error) if missing(&error) => {
            create(store, name).await?;
            connect(store, name).await?
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to connect to database {name}"));
        }
    };
    let migrated = environment.migrate().await;
    environment.close().await;
    migrated.with_context(|| format!("failed to apply the kernel migrations to {name}"))
}

async fn create(store: &PlatformStore, name: &DatabaseName) -> Result<()> {
    // PostgreSQL takes no parameter here and runs the statement only outside a
    // transaction and outside a function, so its text names the validated database.
    let created = sqlx::raw_sql(AssertSqlSafe(format!("CREATE DATABASE {}", name.quoted())))
        .execute(store.pool())
        .await;
    match created {
        Ok(_) => {
            tracing::info!(database = %name, "data environment database created");
            Ok(())
        }
        Err(error) if created_meanwhile(&error) => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to create database {name}")),
    }
}

/// Drop the database `name` when no instance names it, ending its remaining sessions.
/// Returns false, dropping nothing, when an instance names it or it is the
/// installation database.
pub async fn drop_database_if_unused(store: &PlatformStore, name: &DatabaseName) -> Result<bool> {
    if is_installation(store, name) {
        return Ok(false);
    }
    let referenced = sqlx::query_file_scalar!("src/data_environment/referenced.sql", name.as_str())
        .fetch_one(store.pool())
        .await
        .context("failed to look up the instances of the database")?;
    if referenced {
        return Ok(false);
    }
    // As for CREATE DATABASE, the statement takes no parameter.
    sqlx::raw_sql(AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS {} WITH (FORCE)",
        name.quoted()
    )))
    .execute(store.pool())
    .await
    .with_context(|| format!("failed to drop database {name}"))?;
    tracing::info!(database = %name, "data environment database dropped");
    Ok(true)
}

/// The database of `deployment`'s data environment, when the deployment is an
/// instance outside the installation database.
pub(crate) async fn of_deployment(
    store: &PlatformStore,
    deployment: DeploymentId,
) -> Result<Option<DatabaseName>> {
    let database =
        sqlx::query_file_scalar!("src/data_environment/deployment.sql", deployment.as_uuid())
            .fetch_optional(store.pool())
            .await
            .context("failed to read the instance's database")?;
    Ok(database
        .map(DatabaseName::parse)
        .transpose()
        .context("the stored instance database is invalid")?
        .filter(|database| !is_installation(store, database)))
}

/// A pool of one connection to the database `name` with `store`'s host, settings and
/// login. The caller closes it.
pub(crate) async fn connect(store: &PlatformStore, name: &DatabaseName) -> Result<PlatformStore> {
    open(store, name)
        .await
        .with_context(|| format!("failed to connect to database {name}"))
}

async fn open(store: &PlatformStore, name: &DatabaseName) -> Result<PlatformStore, StoreError> {
    let config = store.config();
    let mut url = config.url().clone();
    url.set_path(name.as_str());
    let config = StoreConfig::builder(url.as_str(), config.password().clone())
        .username(config.username())
        .max_connections(1)
        .acquire_timeout(config.acquire_timeout())
        .build()?;
    PlatformStore::connect(config).await
}

/// The database a connection named does not exist: `invalid_catalog_name`.
fn missing(error: &StoreError) -> bool {
    matches!(error, StoreError::Database(error) if has_code(error, &["3D000"]))
}

/// A concurrent creation of the same database won: PostgreSQL reports
/// `duplicate_database` when the name existed before the statement and a unique
/// violation of `pg_database` when another creation committed during it.
fn created_meanwhile(error: &sqlx::Error) -> bool {
    has_code(error, &["42P04", "23505"])
}

/// Whether PostgreSQL answered `error` with one of these SQLSTATE codes.
fn has_code(error: &sqlx::Error, codes: &[&str]) -> bool {
    error
        .as_database_error()
        .and_then(|error| error.code())
        .is_some_and(|code| codes.contains(&code.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_names_are_lowercase_identifiers_of_at_most_63_bytes() {
        for valid in ["veoveo", "dev_jose_x", "0", "_", &"a".repeat(63)] {
            assert_eq!(DatabaseName::parse(valid).unwrap().as_str(), valid);
        }
        for invalid in [
            "",
            "Dev",
            "dev-jose",
            "dev/jose",
            "dev jose",
            "dev\"x",
            "dév",
            &"a".repeat(64),
        ] {
            assert!(DatabaseName::parse(invalid).is_err(), "{invalid:?}");
        }
        assert_eq!(DatabaseName::parse("dev_x").unwrap().quoted(), "\"dev_x\"");
    }
}
