//! Schema migrations through the core module runner. The registry holds the kernel
//! modules and every module a namespace claim recorded, because the runner refuses a
//! database whose history names a module it does not know.

use anyhow::{Context, Result};
use surrealdb::{Connection, Surreal};
use veoveo_modules::{
    DeclarationError, ExecutionCommand, ExecutionImage, LaneExecution, ModuleManifest, ModuleName,
    ModuleRegistry, runner,
};

/// The Gateway runs every lane itself, at startup and when a server registers.
fn execution(_module: &str) -> Result<LaneExecution, DeclarationError> {
    LaneExecution::new(
        ExecutionImage::new("veoveo-service-gateway")?,
        ExecutionCommand::new(vec!["veoveo-service-gateway".into(), "serve".into()])?,
    )
}

/// Kernel modules plus the given server modules, each admitted as a compiled
/// declaration would be.
pub fn module_registry<'a>(
    modules: impl IntoIterator<Item = &'a ModuleManifest>,
) -> Result<ModuleRegistry, DeclarationError> {
    let mut setups = veoveo_platform_store::schema::kernel_modules(execution)?;
    for manifest in modules {
        setups.push(manifest.admit(execution(manifest.name.as_str())?)?);
    }
    ModuleRegistry::new(setups)
}

/// Check that every selected migration passes the runner's SQL admission, without
/// touching the database.
pub fn admit_modules<'a>(modules: impl IntoIterator<Item = &'a ModuleManifest>) -> Result<()> {
    let manifests: Vec<&ModuleManifest> = modules.into_iter().collect();
    let registry = module_registry(manifests.iter().copied())?;
    let selected: Vec<ModuleName> = manifests.iter().map(|m| m.name.clone()).collect();
    runner::prepare(registry.select(selected)?).map_err(anyhow::Error::from)?;
    Ok(())
}

/// Apply every pending migration of the kernel and the `selected` server modules.
/// `known` lists every server module whose history the database may hold.
pub async fn apply_modules<'a, C: Connection>(
    db: &Surreal<C>,
    known: impl IntoIterator<Item = &'a ModuleManifest>,
    selected: Vec<ModuleName>,
) -> Result<()> {
    let registry = module_registry(known)?;
    let prepared = runner::prepare(registry.select(selected)?)?;
    let status = prepared
        .apply(db)
        .await
        .context("module migrations failed")?;
    anyhow::ensure!(status.is_current(), "module lanes are not current");
    Ok(())
}

/// How a newly received manifest relates to the history already applied for its
/// slug. A version may add migrations; it may not change one that was applied.
#[derive(Debug, PartialEq, Eq)]
pub enum ModuleHistory {
    /// The received manifest extends the recorded one (or nothing was recorded).
    Extends,
    /// The recorded manifest already contains every received migration.
    Contained,
    Diverges,
}

pub fn compare_history(
    recorded: Option<&ModuleManifest>,
    received: &ModuleManifest,
) -> ModuleHistory {
    let Some(recorded) = recorded else {
        return ModuleHistory::Extends;
    };
    let shared = recorded.migrations.len().min(received.migrations.len());
    if recorded.migrations[..shared] != received.migrations[..shared] {
        return ModuleHistory::Diverges;
    }
    if received.migrations.len() > recorded.migrations.len() {
        ModuleHistory::Extends
    } else {
        ModuleHistory::Contained
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(sql: &[&str]) -> ModuleManifest {
        serde_json::from_value(serde_json::json!({
            "name": "clock",
            "ownership": [{"kind": "table_prefix", "name": "clock_"}],
            "migrations": sql.iter().enumerate().map(|(version, sql)| serde_json::json!({
                "version": version, "name": format!("step_{version}"), "sql": sql
            })).collect::<Vec<_>>()
        }))
        .unwrap()
    }

    #[test]
    fn a_new_version_may_append_migrations_but_not_rewrite_them() {
        let first = manifest(&["DEFINE TABLE clock_event SCHEMAFULL;"]);
        let second = manifest(&[
            "DEFINE TABLE clock_event SCHEMAFULL;",
            "DEFINE FIELD at ON clock_event TYPE datetime;",
        ]);
        let rewritten = manifest(&["DEFINE TABLE clock_event SCHEMALESS;"]);
        assert_eq!(compare_history(None, &first), ModuleHistory::Extends);
        assert_eq!(
            compare_history(Some(&first), &second),
            ModuleHistory::Extends
        );
        assert_eq!(
            compare_history(Some(&second), &first),
            ModuleHistory::Contained
        );
        assert_eq!(
            compare_history(Some(&first), &rewritten),
            ModuleHistory::Diverges
        );
    }

    #[test]
    fn server_modules_pass_runner_admission_beside_the_kernel() {
        admit_modules([&manifest(&["DEFINE TABLE clock_event SCHEMAFULL;"])]).unwrap();
    }
}
