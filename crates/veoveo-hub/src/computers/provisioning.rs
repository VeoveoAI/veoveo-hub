//! Native workspace/provider/template lifecycle. No grant is persisted by the Hub.
use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use openshell_sdk::raw::proto as p;
use prost_types::Timestamp;
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

use super::gateway::{Gateway, RPC_TIMEOUT};
use crate::{
    HubCatalog, HubSecretResolver, contract::SecretPurpose, control_plane::Computers,
    store::Computer,
};

pub(super) const HUB_PROVIDER: &str = "computer-hub";
pub(super) const MODEL_PROVIDER: &str = "computer-model";
const RETENTION: &str = "openshell.nvidia.com/retention";
pub(super) const READY_TIMEOUT: Duration = Duration::from_secs(120);

pub(super) struct LoginGrant {
    pub access: SecretString,
    pub client: veoveo_hub_sdk::OAuthClientId,
    pub refresh: SecretString,
    pub access_expiry: DateTime<Utc>,
}
struct ProviderSeed<'a> {
    name: &'a str,
    profile: &'a str,
    key: &'a str,
    value: &'a str,
    expiry: Option<DateTime<Utc>>,
    request_id: Uuid,
}
#[derive(Clone)]
pub(super) struct Provisioner {
    pub gateway: Gateway,
    pub config: Arc<Computers>,
    policy: p::sandbox::v1::SandboxPolicy,
    public: String,
    pub spec: super::ComputerProvisionSpec,
}
fn scope(name: &str) -> Option<p::WorkspaceSelector> {
    Some(p::WorkspaceSelector {
        selection: Some(p::workspace_selector::Selection::Workspace(name.into())),
    })
}
fn metadata(name: &str, workspace: &str) -> p::ObjectMeta {
    p::ObjectMeta {
        name: name.into(),
        workspace: workspace.into(),
        ..Default::default()
    }
}
fn request(handoff: Uuid, action: &str) -> String {
    Uuid::new_v5(&handoff, action.as_bytes()).to_string()
}
fn timestamp(value: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: value.timestamp(),
        nanos: i32::try_from(value.timestamp_subsec_nanos()).expect("nanoseconds fit"),
    }
}
fn native_error(operation: &'static str, status: tonic::Status) -> anyhow::Error {
    // Native/provider-controlled descriptions never enter browser responses/logs.
    anyhow::anyhow!("Computer native {operation} failed: {}", status.code())
}
impl Provisioner {
    pub async fn prepare(
        config: Arc<Computers>,
        previous: Option<&Self>,
        catalog: &HubCatalog,
        public: &str,
    ) -> Result<Self> {
        let policy = openshell_policy::validate_and_canonicalize_sandbox_policy(
            openshell_policy::parse_sandbox_policy_file(&config.policy_file)
                .map_err(|_| anyhow::anyhow!("trusted Computer native policy admission failed"))?,
        )
        .context("canonicalize trusted Computer native policy")?;
        let gateway = match previous {
            Some(old) if old.config.gateway == config.gateway => {
                old.gateway.with_limit(config.forwarding_connection_limit)
            }
            _ => Gateway::connect(&config.gateway, config.forwarding_connection_limit).await?,
        };
        let mut grpc = gateway.client.raw_grpc_fresh().await?;
        let info = tokio::time::timeout(
            RPC_TIMEOUT,
            grpc.get_gateway_info(p::GetGatewayInfoRequest {}),
        )
        .await?
        .map_err(|status| {
            native_error(
                "get gateway info (requires config:read and platform_admin)",
                status,
            )
        })?
        .into_inner();
        ensure!(
            info.compute_drivers.len() == 1,
            "Computer requires one selected native driver"
        );
        let template = tokio::time::timeout(
            RPC_TIMEOUT,
            grpc.get_sandbox_template(p::GetSandboxTemplateRequest {
                workspace_scope: scope(&config.template.workspace),
                name: config.template.name.clone(),
            }),
        )
        .await?
        .map_err(|e| native_error("get template", e))?
        .into_inner()
        .template
        .context("native template omitted")?;
        ensure!(
            template
                .spec
                .as_ref()
                .and_then(|v| v.workload.as_ref())
                .is_some_and(|v| !v.image.is_empty()),
            "Computer native image unspecified"
        );
        let backend = super::spec::persistence(&info.compute_drivers[0], &template)?;
        let model_secret = catalog
            .secret_reference(&config.model_provider.secret)
            .context("Computer model secret missing")?
            .clone();
        let _ = HubSecretResolver::new()
            .resolve_reference(&model_secret, SecretPurpose::ProviderApiKey)
            .await?;
        let hub_url = catalog
            .profile(&config.profile)
            .context("Computer profile unavailable")?
            .protected_resource
            .to_string();
        let spec = super::ComputerProvisionSpec::new(
            (*config).clone(),
            model_secret,
            hub_url,
            backend,
            &policy,
            &template,
        );
        let result = Self {
            gateway,
            config,
            policy,
            public: public.to_owned(),
            spec,
        };
        result.validate_profiles(public).await?;
        Ok(result)
    }
    pub fn for_computer(&self, computer: &Computer) -> Result<Self> {
        self.for_spec(computer.provisioning.clone())
    }
    pub fn for_spec(&self, spec: super::ComputerProvisionSpec) -> Result<Self> {
        ensure!(
            spec.backend == self.spec.backend && spec.config.gateway == self.config.gateway,
            "Computer recorded native backend or gateway differs; explicit replacement required"
        );
        Ok(Self {
            // Forwarding capacity is process-wide live policy, not a per-user
            // quota. Old instance governance does not allocate another budget.
            gateway: self.gateway.clone(),
            config: Arc::new(spec.config.clone()),
            policy: spec.policy()?,
            public: self.public.clone(),
            spec,
        })
    }
    async fn validate_profiles(&self, public: &str) -> Result<()> {
        let mut grpc = self.gateway.client.raw_grpc_fresh().await?;
        for (id, key, refresh) in [
            (
                &self.config.hub_provider.profile,
                &self.config.hub_provider.credential_key,
                true,
            ),
            (
                &self.config.model_provider.profile,
                &self.config.model_provider.credential_key,
                false,
            ),
        ] {
            let profile = tokio::time::timeout(
                RPC_TIMEOUT,
                grpc.get_provider_profile(p::GetProviderProfileRequest {
                    id: id.clone(),
                    workspace_scope: None,
                }),
            )
            .await?
            .map_err(|e| native_error("get profile", e))?
            .into_inner()
            .profile
            .context("native profile omitted")?;
            ensure!(
                profile.id == *id && profile.scope == "platform",
                "Computer profile is not global"
            );
            let credential = profile
                .credentials
                .iter()
                .find(|c| c.env_vars.iter().any(|env| env == key))
                .context("Computer profile credential undeclared")?;
            ensure!(
                credential.required
                    && credential.env_vars.len() == 1
                    && credential.auth_style == "bearer"
                    && credential.header_name.eq_ignore_ascii_case("authorization")
                    && credential.token_grant.is_none(),
                "Computer profile credential shape differs from installation"
            );
            if refresh {
                let strategy = credential
                    .refresh
                    .as_ref()
                    .context("Computer Hub profile has no native refresh strategy")?;
                ensure!(
                    strategy.strategy
                        == p::ProviderCredentialRefreshStrategy::Oauth2RefreshToken as i32
                        && strategy.scopes.is_empty()
                        && strategy.token_url
                            == format!("{}/oauth/token", public.trim_end_matches('/'))
                        && !credential.stable_placeholder,
                    "Computer Hub profile cannot own this refresh grant"
                );
            } else {
                ensure!(
                    credential.refresh.is_none(),
                    "Computer static profile must not mint or refresh"
                );
            }
        }
        Ok(())
    }
    async fn source_template(&self) -> Result<p::SandboxWorkloadTemplate> {
        self.spec.template()
    }
    /// Runs only while the caller owns the durable handoff row lock. A bounded
    /// cold-path transaction serializes custody, not an exactly-once claim across
    /// crashes between provider consumption and native durable publication.
    pub async fn handoff(
        &self,
        _catalog: &HubCatalog,
        computer: &Computer,
        grant: LoginGrant,
    ) -> Result<()> {
        // Profile definitions can be updated by native operators. Recheck their
        // actual recipient/strategy before transferring a new user grant.
        self.validate_profiles(&self.public).await?;
        let handoff = computer.handoff.context("Computer handoff missing")?;
        let name = computer.native_name();
        let mut grpc = self.gateway.client.raw_grpc_fresh().await?;
        match tokio::time::timeout(
            RPC_TIMEOUT,
            grpc.get_workspace(p::GetWorkspaceRequest { name: name.clone() }),
        )
        .await?
        {
            Ok(_) => {}
            Err(status) if status.code() == tonic::Code::NotFound => {
                tokio::time::timeout(
                    RPC_TIMEOUT,
                    grpc.create_workspace(p::CreateWorkspaceRequest {
                        name: name.clone(),
                        request_id: computer.workspace_request.to_string(),
                        ..Default::default()
                    }),
                )
                .await?
                .map_err(|e| native_error("create workspace", e))?;
            }
            Err(e) => return Err(native_error("get workspace", e)),
        }
        let mut template = self.source_template().await?;
        template.metadata = Some(metadata(&name, &name));
        let spec = template
            .spec
            .as_mut()
            .context("native template has no spec")?;
        spec.workload
            .as_mut()
            .context("native template has no workload")?
            .environment
            .insert("PUBLIC_BASE_PATH".into(), computer.base_path());
        spec.workload
            .as_mut()
            .context("native template has no workload")?
            .environment
            .insert("COMPUTER_HUB_URL".into(), self.spec.hub_url.clone());
        match tokio::time::timeout(
            RPC_TIMEOUT,
            grpc.get_sandbox_template(p::GetSandboxTemplateRequest {
                workspace_scope: scope(&name),
                name: name.clone(),
            }),
        )
        .await?
        {
            Ok(existing) => ensure!(
                existing.into_inner().template.and_then(|v| v.spec) == template.spec,
                "Computer cloned template differs from installation"
            ),
            Err(status) if status.code() == tonic::Code::NotFound => {
                tokio::time::timeout(
                    RPC_TIMEOUT,
                    grpc.create_sandbox_template(p::CreateSandboxTemplateRequest {
                        workspace_scope: scope(&name),
                        template: Some(template),
                        request_id: computer.template_request.to_string(),
                    }),
                )
                .await?
                .map_err(|e| native_error("clone template", e))?;
            }
            Err(e) => return Err(native_error("get cloned template", e)),
        }
        let model = HubSecretResolver::new()
            .resolve_reference(&self.spec.model_secret, SecretPurpose::ProviderApiKey)
            .await?;
        self.ensure_provider(
            computer,
            ProviderSeed {
                name: MODEL_PROVIDER,
                profile: &self.config.model_provider.profile,
                key: &self.config.model_provider.credential_key,
                value: model.expose_secret(),
                expiry: None,
                request_id: computer.model_provider_request,
            },
        )
        .await?;
        self.ensure_provider(
            computer,
            ProviderSeed {
                name: HUB_PROVIDER,
                profile: &self.config.hub_provider.profile,
                key: &self.config.hub_provider.credential_key,
                value: grant.access.expose_secret(),
                expiry: Some(grant.access_expiry),
                request_id: computer.hub_provider_request,
            },
        )
        .await?;

        let existing = self.sandbox(&name).await?;
        if let Some(sandbox) = &existing {
            self.validate_sandbox(computer, sandbox)?;
            let phase = phase(sandbox);
            if phase != p::SandboxPhase::Stopped && phase != p::SandboxPhase::Completed {
                tokio::time::timeout(
                    RPC_TIMEOUT,
                    grpc.stop_sandbox(p::StopSandboxRequest {
                        workspace_scope: scope(&name),
                        name: name.clone(),
                        request_id: request(handoff, "stop"),
                    }),
                )
                .await?
                .map_err(|e| native_error("stop before custody", e))?;
                self.wait_phase(&name, p::SandboxPhase::Stopped).await?;
            }
        }
        tokio::time::timeout(
            RPC_TIMEOUT,
            grpc.configure_provider_refresh(p::ConfigureProviderRefreshRequest {
                workspace_scope: scope(&name),
                provider: HUB_PROVIDER.into(),
                credential_key: self.config.hub_provider.credential_key.clone(),
                strategy: p::ProviderCredentialRefreshStrategy::Oauth2RefreshToken as i32,
                material: HashMap::from([
                    ("client_id".into(), grant.client.to_string()),
                    ("refresh_token".into(), grant.refresh.expose_secret().into()),
                ]),
                secret_material_keys: vec!["refresh_token".into()],
                expiration_time: Some(timestamp(grant.access_expiry)),
                request_id: computer
                    .configure_request
                    .context("Computer configure ID missing")?
                    .to_string(),
            }),
        )
        .await?
        .map_err(|e| native_error("configure refresh custody", e))?;
        let rotated = tokio::time::timeout(
            RPC_TIMEOUT,
            grpc.rotate_provider_credential(p::RotateProviderCredentialRequest {
                workspace_scope: scope(&name),
                provider: HUB_PROVIDER.into(),
                credential_key: self.config.hub_provider.credential_key.clone(),
                request_id: computer
                    .rotate_request
                    .context("Computer rotate ID missing")?
                    .to_string(),
            }),
        )
        .await?
        .map_err(|e| native_error("rotate user credential", e))?
        .into_inner()
        .status
        .context("native rotation omitted status")?;
        ensure!(
            rotated
                .expiration_time
                .is_some_and(|expiry| expiry.seconds > Utc::now().timestamp())
                && rotated.failure_code.is_empty(),
            "Computer credential rotation not ready"
        );
        drop(grant);
        if existing.is_none() {
            tokio::time::timeout(
                RPC_TIMEOUT,
                grpc.create_sandbox(p::CreateSandboxRequest {
                    workspace_scope: scope(&name),
                    name: name.clone(),
                    workload_template: name.clone(),
                    request_id: computer.sandbox_request.to_string(),
                    annotations: HashMap::from([(RETENTION.into(), "persistent".into())]),
                    spec: Some(p::SandboxSpec {
                        policy: Some(self.policy.clone()),
                        command: self.config.command.clone(),
                        providers: vec![HUB_PROVIDER.into(), MODEL_PROVIDER.into()],
                        restart_policy: p::SandboxRestartPolicy::Always as i32,
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            )
            .await?
            .map_err(|e| native_error("create sandbox", e))?;
        } else {
            tokio::time::timeout(
                RPC_TIMEOUT,
                grpc.start_sandbox(p::StartSandboxRequest {
                    workspace_scope: scope(&name),
                    name: name.clone(),
                    request_id: request(handoff, "start"),
                }),
            )
            .await?
            .map_err(|e| native_error("start sandbox", e))?;
        }
        self.wait_phase(&name, p::SandboxPhase::Ready).await?;
        self.wait_listener(&name).await
    }
    async fn ensure_provider(&self, computer: &Computer, seed: ProviderSeed<'_>) -> Result<()> {
        let ProviderSeed {
            name,
            profile,
            key,
            value,
            expiry,
            request_id,
        } = seed;
        let workspace = computer.native_name();
        let mut grpc = self.gateway.client.raw_grpc_fresh().await?;
        match tokio::time::timeout(
            RPC_TIMEOUT,
            grpc.get_provider(p::GetProviderRequest {
                workspace_scope: scope(&workspace),
                name: name.into(),
            }),
        )
        .await?
        {
            Ok(response) => {
                let provider = response
                    .into_inner()
                    .provider
                    .context("native provider omitted")?;
                ensure!(
                    provider.r#type == profile
                        && provider.profile_workspace.is_empty()
                        && provider
                            .metadata
                            .as_ref()
                            .is_some_and(|m| m.name == name && m.workspace == workspace)
                        && provider.config.is_empty()
                        && provider.credentials.contains_key(key),
                    "Computer provider differs from trusted installation"
                );
            }
            Err(status) if status.code() == tonic::Code::NotFound => {
                let profile = tokio::time::timeout(
                    RPC_TIMEOUT,
                    grpc.get_provider_profile(p::GetProviderProfileRequest {
                        id: profile.into(),
                        workspace_scope: None,
                    }),
                )
                .await?
                .map_err(|e| native_error("get provider profile", e))?
                .into_inner()
                .profile
                .context("native profile omitted")?;
                let env = profile
                    .credentials
                    .iter()
                    .find(|c| c.env_vars.iter().any(|env| env == key))
                    .and_then(|c| c.env_vars.first())
                    .context("native credential env undeclared")?
                    .clone();
                tokio::time::timeout(
                    RPC_TIMEOUT,
                    grpc.create_provider(p::CreateProviderRequest {
                        workspace_scope: scope(&workspace),
                        request_id: request_id.to_string(),
                        provider: Some(p::Provider {
                            metadata: Some(metadata(name, &workspace)),
                            r#type: profile.id,
                            credentials: HashMap::from([(env.clone(), value.into())]),
                            credential_expiration_times: expiry
                                .map(|e| HashMap::from([(env, timestamp(e))]))
                                .unwrap_or_default(),
                            ..Default::default()
                        }),
                    }),
                )
                .await?
                .map_err(|e| native_error("create provider", e))?;
            }
            Err(e) => return Err(native_error("get provider", e)),
        }
        Ok(())
    }
    /// Browser entry needs a closed probe because it only redirects afterward.
    pub async fn ensure_running(&self, computer: &Computer) -> Result<()> {
        self.resume(computer).await?;
        self.wait_listener(&computer.native_name()).await
    }

    /// Native lifecycle recovery for document navigation. Its actual forwarding
    /// connection performs the bounded listener wait, without an extra probe.
    pub async fn resume(&self, computer: &Computer) -> Result<()> {
        let name = computer.native_name();
        let sandbox = self
            .sandbox(&name)
            .await?
            .context("Computer native sandbox missing; reauthorize")?;
        self.validate_sandbox(computer, &sandbox)?;
        match phase(&sandbox) {
            p::SandboxPhase::Ready => Ok(()),
            p::SandboxPhase::Deleting => anyhow::bail!("Computer deletion in progress"),
            p::SandboxPhase::Starting | p::SandboxPhase::Provisioning => {
                self.wait_phase(&name, p::SandboxPhase::Ready).await
            }
            _ => {
                let mut grpc = self.gateway.client.raw_grpc_fresh().await?;
                tokio::time::timeout(
                    RPC_TIMEOUT,
                    grpc.start_sandbox(p::StartSandboxRequest {
                        workspace_scope: scope(&name),
                        name: name.clone(),
                        request_id: Uuid::now_v7().to_string(),
                    }),
                )
                .await?
                .map_err(|e| native_error("resume sandbox", e))?;
                self.wait_phase(&name, p::SandboxPhase::Ready).await
            }
        }
    }
    fn validate_sandbox(&self, computer: &Computer, sandbox: &p::Sandbox) -> Result<()> {
        let name = computer.native_name();
        let spec = sandbox
            .spec
            .as_ref()
            .context("native Computer spec missing")?;
        ensure!(
            sandbox.metadata.as_ref().is_some_and(|m| m.name == name
                && m.workspace == name
                && m.annotations.get(RETENTION).map(String::as_str) == Some("persistent"))
                && sandbox
                    .created_from_workload_template
                    .as_ref()
                    .is_some_and(|v| v.name == name)
                && spec.command == self.config.command
                && spec.policy.as_ref() == Some(&self.policy)
                && spec.providers == [HUB_PROVIDER.to_owned(), MODEL_PROVIDER.to_owned()]
                && spec.restart_policy == p::SandboxRestartPolicy::Always as i32
                && spec.environment.get("PUBLIC_BASE_PATH") == Some(&computer.base_path()),
            "native Computer governance differs from installation"
        );
        Ok(())
    }
    async fn sandbox(&self, name: &str) -> Result<Option<p::Sandbox>> {
        let mut grpc = self.gateway.client.raw_grpc_fresh().await?;
        match tokio::time::timeout(
            RPC_TIMEOUT,
            grpc.get_sandbox(p::GetSandboxRequest {
                workspace_scope: scope(name),
                name: name.into(),
            }),
        )
        .await?
        {
            Ok(response) => Ok(Some(
                response
                    .into_inner()
                    .sandbox
                    .context("native sandbox omitted")?,
            )),
            Err(status) if status.code() == tonic::Code::NotFound => Ok(None),
            Err(e) => Err(native_error("get sandbox", e)),
        }
    }
    async fn wait_listener(&self, name: &str) -> Result<()> {
        let (socket, task) = self
            .gateway
            .stream_when_ready(name.to_owned(), self.config.port, READY_TIMEOUT)
            .await?;
        drop(socket);
        task.abort();
        let _ = task.await;
        Ok(())
    }
    async fn wait_phase(&self, name: &str, expected: p::SandboxPhase) -> Result<()> {
        tokio::time::timeout(READY_TIMEOUT, async {
            loop {
                let sandbox = self
                    .sandbox(name)
                    .await?
                    .context("native Computer disappeared during transition")?;
                let observed = phase(&sandbox);
                if observed == expected {
                    return Ok(());
                }
                ensure!(
                    observed != p::SandboxPhase::Deleting
                        && (expected == p::SandboxPhase::Stopped
                            || observed != p::SandboxPhase::Error),
                    "native Computer transition failed"
                );
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
        .await
        .context("Computer native transition deadline")?
    }
}
fn phase(sandbox: &p::Sandbox) -> p::SandboxPhase {
    p::SandboxPhase::try_from(sandbox.status.as_ref().map_or(0, |status| status.phase))
        .unwrap_or(p::SandboxPhase::Unknown)
}
