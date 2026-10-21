//! Reusable native runtime identity and exact per-build listener placement. This is
//! resume provenance, not another native sandbox inventory or lifecycle controller.
use anyhow::{Context, Result, ensure};
use openshell_sdk::{ExecOptions, OpenShellClient, raw::proto as p};
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Connection as _, PgConnection, types::Json};
use std::{collections::HashMap, time::Duration};
use veoveo_hub::{
    HubCatalog,
    activation::{AcceptedApply, ActivationError},
};
use veoveo_types::BuildId;
#[path = "../../environment/protocol.rs"]
pub(super) mod protocol;

const TIMEOUT: Duration = Duration::from_secs(30);
const HELPER: &str = "/usr/local/bin/veoveo-environment";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RuntimeIdentity {
    pub name: String,
    pub workspace: String,
    pub native_id: String,
    pub template: Vec<u8>,
    pub template_resource_version: String,
    pub policy: Vec<u8>,
    pub providers: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct WorkerPlacement {
    pub runtime: RuntimeIdentity,
    pub port: u16,
}

#[derive(Debug, thiserror::Error)]
#[error("native execution transport unavailable")]
pub(super) struct NativeUnavailable;

pub(super) struct Runtime {
    pub client: OpenShellClient,
    pub identity: RuntimeIdentity,
}
fn scope(workspace: &str) -> Option<p::WorkspaceSelector> {
    Some(p::workspace_selector(workspace))
}
fn refusal(operation: &'static str, status: tonic::Status) -> anyhow::Error {
    if matches!(
        status.code(),
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded
    ) {
        anyhow::anyhow!(NativeUnavailable)
    } else {
        anyhow::anyhow!("native activation {operation} refused: {}", status.code())
    }
}
/// Retry only concrete native transport failures. Invalid policy/metadata, auth,
/// helper nonzero exit and application readiness failure remain explicit failures.
pub(super) fn transport_failure(error: &anyhow::Error) -> bool {
    if error.is::<NativeUnavailable>() || error.is::<tonic::transport::Error>() {
        return true;
    }
    if let Some(status) = error.downcast_ref::<tonic::Status>() {
        return matches!(
            status.code(),
            tonic::Code::Unavailable | tonic::Code::DeadlineExceeded
        );
    }
    if let Some(error) = error.downcast_ref::<openshell_sdk::SdkError>() {
        return matches!(
            error,
            openshell_sdk::SdkError::Connect { .. }
                | openshell_sdk::SdkError::Auth {
                    retryable: true,
                    ..
                }
        ) || error.grpc_status().is_some_and(|status| {
            matches!(
                status.code(),
                tonic::Code::Unavailable | tonic::Code::DeadlineExceeded
            )
        });
    }
    false
}
impl Runtime {
    fn name(accepted: &AcceptedApply) -> Result<(String, [u8; 32])> {
        const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
        let key = serde_json::to_vec(&(&accepted.key, &accepted.inputs.runtime_template))?;
        let hash: [u8; 32] = Sha256::digest(key).into();
        // Native routable names allow 19 characters. Retain 90 hash bits in
        // lowercase base32; the full digest still binds the create request ID.
        let mut name = String::with_capacity(19);
        name.push('e');
        for bit in (0..90).step_by(5) {
            let byte = bit / 8;
            let window = (u16::from(hash[byte]) << 8) | u16::from(hash[byte + 1]);
            let index = usize::from((window >> (11 - bit % 8)) & 31);
            name.push(char::from(ALPHABET[index]));
        }
        Ok((name, hash))
    }
    pub async fn existing(
        catalog: &HubCatalog,
        connection: &mut PgConnection,
        accepted: &AcceptedApply,
    ) -> Result<Self> {
        accepted.admit(catalog.control_plane())?;
        Self::load(catalog, connection, accepted).await
    }
    pub async fn retained(
        catalog: &HubCatalog,
        connection: &mut PgConnection,
        accepted: &AcceptedApply,
    ) -> Result<Self> {
        accepted.admit_retained(catalog.control_plane())?;
        Self::load(catalog, connection, accepted).await
    }
    async fn load(
        catalog: &HubCatalog,
        connection: &mut PgConnection,
        accepted: &AcceptedApply,
    ) -> Result<Self> {
        let (name, _) = Self::name(accepted)?;
        let row = sqlx::query_file!("src/bin/hub/activation/native/runtime_read.sql", name)
            .fetch_optional(connection)
            .await?
            .context("native runtime has not been prepared")?;
        Ok(Self {
            client: catalog
                .native_control_client()
                .context("native runtime gateway unavailable")?,
            identity: row.identity.0,
        })
    }
    /// Called while the activation phase's existing Singleton role is owned.
    /// Native create request IDs are deterministic per owned template/runtime.
    pub async fn ensure(
        catalog: &HubCatalog,
        connection: &mut PgConnection,
        accepted: &AcceptedApply,
    ) -> Result<Self> {
        accepted.admit(catalog.control_plane())?;
        let client = catalog
            .native_control_client()
            .context("native activation gateway is unavailable")?;
        let mut grpc = client.raw_grpc_fresh().await?;
        let template = tokio::time::timeout(
            TIMEOUT,
            grpc.get_sandbox_template(p::GetSandboxTemplateRequest {
                workspace_scope: scope(&accepted.runtime.workspace),
                name: accepted.runtime.template.clone(),
            }),
        )
        .await
        .map_err(|_| NativeUnavailable)?
        .map_err(|e| refusal("get template", e))?
        .into_inner()
        .template
        .context("native runtime template omitted")?;
        let spec = template
            .spec
            .clone()
            .context("native runtime template spec omitted")?;
        let template_bytes = spec.encode_to_vec();
        let template_resource_version = template
            .metadata
            .as_ref()
            .context("native template metadata missing")?
            .resource_version;
        ensure!(
            template_resource_version > 0,
            "native template resource version missing"
        );
        let template_resource_version = template_resource_version.to_string();
        let policy = openshell_policy::validate_and_canonicalize_sandbox_policy(
            openshell_policy::parse_sandbox_policy_file(&accepted.runtime.policy_file)
                .map_err(|_| anyhow::anyhow!("trusted runtime policy failed admission"))?,
        )?;
        let policy_bytes = policy.encode_to_vec();
        let (name, hash) = Self::name(accepted)?;
        let request_id = uuid::Uuid::new_v5(&accepted.key.environment.as_uuid(), &hash).to_string();
        let previous = sqlx::query_file!("src/bin/hub/activation/native/runtime_read.sql", name)
            .fetch_optional(&mut *connection)
            .await?;
        if let Some(previous) = &previous {
            ensure!(
                p::SandboxWorkloadTemplateSpec::decode(previous.identity.0.template.as_slice())?
                    == spec
                    && p::sandbox::v1::SandboxPolicy::decode(
                        previous.identity.0.policy.as_slice()
                    )? == policy
                    && previous.identity.0.workspace == accepted.runtime.workspace
                    && previous.identity.0.template_resource_version == template_resource_version
                    && previous.identity.0.providers == accepted.runtime.providers,
                "native runtime template/policy changed under a versioned reference"
            );
        }
        let found = match tokio::time::timeout(
            TIMEOUT,
            grpc.get_sandbox(p::GetSandboxRequest {
                workspace_scope: scope(&accepted.runtime.workspace),
                name: name.clone(),
            }),
        )
        .await
        .map_err(|_| NativeUnavailable)?
        {
            Ok(response) => response
                .into_inner()
                .sandbox
                .context("native runtime omitted")?,
            Err(status) if status.code() == tonic::Code::NotFound && previous.is_none() => {
                tokio::time::timeout(
                    TIMEOUT,
                    grpc.create_sandbox(p::CreateSandboxRequest {
                        workspace_scope: scope(&accepted.runtime.workspace),
                        name: name.clone(),
                        workload_template: accepted.runtime.template.clone(),
                        request_id,
                        annotations: HashMap::from([(
                            "openshell.nvidia.com/retention".into(),
                            "persistent".into(),
                        )]),
                        spec: Some(p::SandboxSpec {
                            policy: Some(policy.clone()),
                            providers: accepted.runtime.providers.clone(),
                            command: vec!["/usr/bin/s6-svscan".into(), "/sandbox/services".into()],
                            restart_policy: p::SandboxRestartPolicy::Always as i32,
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                )
                .await
                .map_err(|_| NativeUnavailable)?
                .map_err(|e| refusal("create runtime", e))?
                .into_inner()
                .sandbox
                .context("created runtime omitted")?
            }
            Err(status) => return Err(refusal("get runtime", status)),
        };
        let found_spec = found.spec.as_ref().context("native runtime spec omitted")?;
        ensure!(
            found.metadata.as_ref().is_some_and(|m| m.name == name
                && m.workspace == accepted.runtime.workspace
                && m.annotations
                    .get("openshell.nvidia.com/retention")
                    .map(String::as_str)
                    == Some("persistent"))
                && found
                    .created_from_workload_template
                    .as_ref()
                    .is_some_and(|t| t.name == accepted.runtime.template
                        && t.resource_version == template_resource_version)
                && found_spec.policy.as_ref() == Some(&policy)
                && found_spec.providers == accepted.runtime.providers
                && found_spec.command == ["/usr/bin/s6-svscan", "/sandbox/services"]
                && found_spec.restart_policy == p::SandboxRestartPolicy::Always as i32,
            "native runtime governance differs from admitted template"
        );
        let mut identity = RuntimeIdentity {
            name,
            workspace: accepted.runtime.workspace.clone(),
            native_id: found
                .metadata
                .as_ref()
                .context("native runtime metadata missing")?
                .id
                .clone(),
            template: template_bytes,
            template_resource_version,
            policy: policy_bytes,
            providers: accepted.runtime.providers.clone(),
        };
        if let Some(previous) = previous {
            ensure!(
                previous.identity.0.native_id == identity.native_id
                    && previous.identity.0.name == identity.name,
                "owned runtime identity differs from admitted native resource"
            );
            identity = previous.identity.0;
        }
        sqlx::query_file!(
            "src/bin/hub/activation/native/runtime_insert.sql",
            identity.name,
            Json(&identity) as _
        )
        .execute(&mut *connection)
        .await?;
        Ok(Self { client, identity })
    }
    pub async fn wait_ready(&self) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(120), async {
            loop {
                let sandbox = self
                    .client
                    .workspace(&self.identity.workspace)
                    .get_sandbox(&self.identity.name)
                    .await?;
                ensure!(
                    sandbox.id == self.identity.native_id,
                    "native runtime identity changed"
                );
                match sandbox.phase {
                    openshell_sdk::types::SandboxPhase::Ready => return Ok(()),
                    openshell_sdk::types::SandboxPhase::Provisioning
                    | openshell_sdk::types::SandboxPhase::Starting => {
                        tokio::time::sleep(Duration::from_millis(250)).await
                    }
                    _ => anyhow::bail!(
                        "native runtime not running; explicit lifecycle recovery required"
                    ),
                }
            }
        })
        .await
        .map_err(|_| NativeUnavailable)?
    }
    pub async fn invoke<T: Serialize, U: serde::de::DeserializeOwned>(
        &self,
        operation: &'static str,
        input: &T,
    ) -> Result<U> {
        let result = self
            .client
            .workspace(&self.identity.workspace)
            .exec(
                &self.identity.name,
                &[HELPER.into(), operation.into()],
                ExecOptions {
                    stdin: Some(serde_json::to_vec(input)?),
                    timeout: Some(Duration::from_secs(match operation {
                        "build" => 900,
                        "materialize" | "materialize-lore" => 180,
                        _ => 30,
                    })),
                    no_login_shell: true,
                    ..Default::default()
                },
            )
            .await
            .map_err(anyhow::Error::from)?;
        ensure!(
            result.exit_code == 0,
            "native activation helper {operation} failed"
        );
        ensure!(
            result.stdout.len() <= 2 * 1024 * 1024,
            "native activation helper result exceeds bound"
        );
        serde_json::from_slice(&result.stdout).context("decode typed native activation result")
    }
    pub async fn retained_placement(
        &self,
        connection: &mut PgConnection,
        build: BuildId,
    ) -> Result<WorkerPlacement> {
        let placement = sqlx::query_file!(
            "src/bin/hub/activation/native/worker_read.sql",
            build.as_uuid()
        )
        .fetch_one(connection)
        .await?
        .placement
        .0;
        ensure!(
            placement.runtime == self.identity,
            "retained worker native placement differs"
        );
        Ok(placement)
    }
    pub async fn placement(
        &self,
        connection: &mut PgConnection,
        build: BuildId,
    ) -> Result<WorkerPlacement> {
        let id = build.as_uuid();
        let mut tx = connection.begin().await?;
        sqlx::query_file!(
            "src/bin/hub/activation/native/runtime_lock.sql",
            self.identity.name
        )
        .fetch_one(&mut *tx)
        .await?;
        if let Some(existing) =
            sqlx::query_file!("src/bin/hub/activation/native/worker_read.sql", id)
                .fetch_optional(&mut *tx)
                .await?
        {
            ensure!(
                existing.placement.0.runtime == self.identity,
                ActivationError::InvalidTransition
            );
            tx.commit().await?;
            return Ok(existing.placement.0);
        }
        let port = sqlx::query_file!("src/bin/hub/activation/native/port.sql", self.identity.name)
            .fetch_optional(&mut *tx)
            .await?
            .context("native runtime worker ports exhausted")?
            .port
            .context("worker port missing")?;
        let placement = WorkerPlacement {
            runtime: self.identity.clone(),
            port: u16::try_from(port)?,
        };
        sqlx::query_file!(
            "src/bin/hub/activation/native/worker_insert.sql",
            id,
            self.identity.name,
            i32::from(placement.port),
            Json(&placement) as _
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(placement)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_names_fit_native_contract_and_bind_environment_component_and_template() {
        let accepted: AcceptedApply = serde_json::from_value(serde_json::json!({
            "runtime":{"workspace":"fixture","template":"fixture-v1","policyFile":"/fixture.yaml","providers":[],"storage":{"kind":"shared_logical_schema","server":"observatory"}},
            "key":{"owner":{"tenant":"fixture","principal":"owner"},"environment":"019a5b30-0000-7000-8000-000000000001","component":"observatory"},
            "generation":1,"build":BuildId::new(),"deployment":veoveo_hub_sdk::DeploymentId::new(),"idempotency":"fixture",
            "inputs":{"runtimeTemplate":"fixture-v1","definitionDigest":veoveo_types::Sha256Digest::from_bytes([1;32]),"expectedSource":null,"recipe":{"worker":{"kind":"compiled","output":{"root":"source","path":"service"},"args":[],"cwd":"."}}}
        })).unwrap();
        let identity = Runtime::name(&accepted).unwrap();
        // Fixed vector independently derived from the unchanged serialized key.
        assert_eq!(identity.0, "envkplxxczljk3ou7c6");
        assert_eq!(
            hex::encode(identity.1),
            "6d54f5dee2cad2adba9f17b24ceba8129081abc942b5089f3c3d1927d0374b0e"
        );
        assert_eq!(Runtime::name(&accepted).unwrap(), identity);
        let mut next_build = accepted.clone();
        next_build.build = BuildId::new();
        next_build.deployment = veoveo_hub_sdk::DeploymentId::new();
        assert_eq!(Runtime::name(&next_build).unwrap(), identity);

        let mut identities = std::collections::BTreeSet::from([identity.0]);
        for changed in ["environment", "component", "template"] {
            let mut candidate = accepted.clone();
            match changed {
                "environment" => {
                    candidate.key.environment =
                        veoveo_hub_sdk::EnvironmentId::parse("019a5b30-0000-7000-8000-000000000002")
                            .unwrap()
                }
                "component" => {
                    candidate.key.component =
                        veoveo_hub_sdk::ServerSlug::parse("fixture-two").unwrap()
                }
                "template" => {
                    candidate.inputs.runtime_template =
                        veoveo_hub::activation::RuntimeTemplateRef::parse("fixture-v2").unwrap()
                }
                _ => unreachable!(),
            }
            let (name, hash) = Runtime::name(&candidate).unwrap();
            assert_eq!(name.len(), 19);
            assert!(name.starts_with('e'));
            assert!(
                name.bytes()
                    .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b))
            );
            assert_ne!(hash, identity.1);
            assert!(
                identities.insert(name),
                "{changed} must select a distinct runtime"
            );
        }
    }
    #[test]
    fn reconnect_does_not_retry_policy_auth_or_candidate_failure() {
        assert!(transport_failure(&anyhow::Error::new(
            openshell_sdk::SdkError::connect("gateway unavailable")
        )));
        assert!(transport_failure(&anyhow::Error::new(
            tonic::Status::unavailable("native restart")
        )));
        assert!(!transport_failure(&anyhow::Error::new(
            tonic::Status::permission_denied("current policy")
        )));
        assert!(!transport_failure(&anyhow::Error::new(
            openshell_sdk::SdkError::invalid_config("template invalid")
        )));
        assert!(!transport_failure(&anyhow::anyhow!(
            "candidate offline metadata mismatch"
        )));
    }
}
