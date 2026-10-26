//! Native workspace/provider/template lifecycle. No grant is persisted by the Hub.
//!
//! [`Installation`] holds what a control revision fixes: the gateway, the policy file's
//! policy, the model secret and the Hub's resource URL. It reads the installation's
//! template when a Computer is entered, compared by its object identity and resource
//! version, and prepares the current version only when that changes; a missing or
//! invalid template leaves Computers unavailable without stopping the Hub.
//! [`Provisioner`] acts on one Computer at one recorded version: the custody handoff of
//! a sign-in, and the replacement of its sandbox, which deletes the sandbox keeping its
//! home, creates one of that version and waits until it is ready.
use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use openshell_sdk::raw::proto::{self as p, ProviderCredentialRefreshStrategy as Strategy};
use prost_types::Timestamp;
use secrecy::{ExposeSecret, SecretString};
use tokio::{sync::Mutex, time::Instant};
use uuid::Uuid;

use super::gateway::{Gateway, RPC_TIMEOUT};
use crate::{
    HubCatalog, HubSecretResolver,
    contract::{SecretPurpose, SecretReference},
    control_plane::Computers,
    custody_client::CustodyClient,
    store::Computer,
};

pub(super) const HUB_PROVIDER: &str = "computer-hub";
pub(super) const MODEL_PROVIDER: &str = "computer-model";
/// The owner's Lore access: tokens the gateway refreshes with the Computer's own Lore
/// client, the owner's login token or one repository's token.
pub(super) const LORE_PROVIDER: &str = "computer-lore";
/// The providers every Computer's sandbox attaches.
const PROVIDERS: [&str; 3] = [HUB_PROVIDER, MODEL_PROVIDER, LORE_PROVIDER];
const RETENTION: &str = "openshell.nvidia.com/retention";
/// The version a sandbox was created at, in its environment beside `LORE_IDENTITY`.
pub(super) const COMPUTER_VERSION: &str = "COMPUTER_VERSION";
pub(super) const READY_TIMEOUT: Duration = Duration::from_secs(120);
/// How long a deleted sandbox may take to go. Kubernetes stops its Pods within their
/// grace period, which slow nodes have outlived several times.
const DELETE_TIMEOUT: Duration = Duration::from_secs(if cfg!(test) { 3 } else { 300 });
/// How long a delete's answer is awaited. The gateway answers once the sandbox is gone,
/// which a Pod's stop can make longer than one request, and it finishes a delete it has
/// taken even when the request ends; after this the sandbox's absence is awaited.
const DELETE_ANSWER: Duration = if cfg!(test) {
    Duration::from_millis(500)
} else {
    RPC_TIMEOUT
};
/// Entries within this long of the last template read share it; tests read it each time.
const TEMPLATE_RECHECK: Duration = Duration::from_secs(if cfg!(test) { 0 } else { 5 });

/// Why Computers cannot start now: the installation's template or a provider profile
/// is missing or invalid, or the gateway cannot answer. A Computer whose sandbox runs is
/// still served.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct Unavailable(pub String);

pub(super) struct LoginGrant {
    pub access: SecretString,
    pub client: veoveo_hub_sdk::OAuthClientId,
    pub refresh: SecretString,
    pub access_expiry: DateTime<Utc>,
    /// The Computer's own Lore client with the new secret whose digest the handoff's
    /// beginning recorded.
    pub lore: CustodyClient,
}
struct ProviderSeed<'a> {
    name: &'a str,
    profile: &'a str,
    key: &'a str,
    /// The credential the provider is created with; none for one the gateway mints.
    value: Option<&'a str>,
    expiry: Option<DateTime<Utc>>,
    request_id: Uuid,
}

/// The current version, or why there is none, as of one read of the template.
struct Current {
    /// The template's object ID and resource version. OpenShell has no template update:
    /// a changed template is a new object, whose resource version starts again at 1.
    template: Option<(String, u64)>,
    checked: Instant,
    result: Result<Provisioner, Unavailable>,
}

#[derive(Clone)]
pub(super) struct Installation {
    pub gateway: Gateway,
    pub config: Arc<Computers>,
    policy: p::sandbox::v1::SandboxPolicy,
    public: String,
    model_secret: SecretReference,
    hub_url: String,
    current: Arc<Mutex<Option<Current>>>,
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
pub(super) fn metadata(name: &str, workspace: &str) -> p::ObjectMeta {
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
/// A refused create. OpenShell refuses it before any change while the previous
/// sandbox of the Computer's name still stops, by its reason, not its message;
/// a restart then succeeds once it has stopped.
fn create_refused(status: tonic::Status) -> anyhow::Error {
    let reason = openshell_sdk::SdkError::from_status(status.clone()).reason();
    if reason.as_deref() == Some(openshell_sdk::error::PREVIOUS_SANDBOX_TERMINATING) {
        anyhow::anyhow!("the previous Computer is still stopping; try again")
    } else {
        native_error("create sandbox", status)
    }
}
impl Installation {
    /// What does not depend on the template: the policy file, the gateway, the model
    /// secret and the profile's resource URL. Reads no template, so a missing one
    /// leaves the Hub running.
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
        Ok(Self {
            gateway,
            config,
            policy,
            public: public.to_owned(),
            model_secret,
            hub_url,
            current: Arc::default(),
        })
    }
    /// An installation of already admitted parts, such as a test's fake gateway.
    #[cfg(test)]
    pub(super) fn of(provisioner: &Provisioner) -> Self {
        Self {
            gateway: provisioner.gateway.clone(),
            config: provisioner.config.clone(),
            policy: provisioner.policy.clone(),
            public: provisioner.public.clone(),
            model_secret: provisioner.spec.model_secret.clone(),
            hub_url: provisioner.spec.hub_url.clone(),
            current: Arc::default(),
        }
    }
    pub fn model_secret(&self) -> &SecretReference {
        &self.model_secret
    }
    pub fn hub_url(&self) -> &str {
        &self.hub_url
    }
    pub fn for_computer(&self, computer: &Computer) -> Result<Provisioner> {
        self.for_spec(computer.provisioning.clone())
    }
    pub fn for_spec(&self, spec: super::ComputerProvisionSpec) -> Result<Provisioner> {
        ensure!(
            spec.config.gateway == self.config.gateway,
            "Computer recorded native gateway differs; explicit replacement required"
        );
        Ok(Provisioner {
            // Forwarding capacity is process-wide live policy, not a per-user
            // quota. Old instance governance does not allocate another budget.
            gateway: self.gateway.clone(),
            config: Arc::new(spec.config.clone()),
            policy: spec.policy()?,
            public: self.public.clone(),
            spec,
        })
    }
    /// The current version. Reads the installation's template, at most once within
    /// [`TEMPLATE_RECHECK`] across entries, and prepares the version again only when
    /// the template is another object or another resource version of it.
    pub async fn current(&self) -> Result<Provisioner, Unavailable> {
        let mut current = self.current.lock().await;
        if let Some(known) = current.as_ref()
            && known.checked.elapsed() < TEMPLATE_RECHECK
        {
            return known.result.clone();
        }
        let template = match self.read_template().await {
            Ok(template) => template,
            Err(unavailable) => {
                let result = Err(unavailable);
                log_change(current.as_ref().map(|known| &known.result), &result);
                *current = Some(Current {
                    template: None,
                    checked: Instant::now(),
                    result: result.clone(),
                });
                return result;
            }
        };
        let identity = template
            .metadata
            .as_ref()
            .map(|metadata| (metadata.id.clone(), metadata.resource_version));
        if let Some(known) = current.as_mut()
            && known.template.is_some()
            && known.template == identity
        {
            known.checked = Instant::now();
            return known.result.clone();
        }
        let result = self.version(template).await.map_err(|error| {
            Unavailable(format!(
                "the installation's Computer template `{}` is not usable: {error:#}",
                self.config.template.name
            ))
        });
        log_change(current.as_ref().map(|known| &known.result), &result);
        *current = Some(Current {
            template: identity,
            checked: Instant::now(),
            result: result.clone(),
        });
        result
    }
    async fn read_template(&self) -> Result<p::SandboxWorkloadTemplate, Unavailable> {
        let name = &self.config.template;
        let unavailable = |reason: String| {
            Unavailable(format!(
                "the installation's Computer template `{}` in workspace `{}` {reason}",
                name.name, name.workspace
            ))
        };
        let mut grpc = self
            .gateway
            .client
            .raw_grpc_fresh()
            .await
            .map_err(|_| unavailable("cannot be read: the gateway is unreachable".into()))?;
        let answer = tokio::time::timeout(
            RPC_TIMEOUT,
            grpc.get_sandbox_template(p::GetSandboxTemplateRequest {
                workspace_scope: scope(&name.workspace),
                name: name.name.clone(),
            }),
        )
        .await
        .map_err(|_| unavailable("cannot be read: the gateway did not answer".into()))?;
        match answer {
            Ok(response) => response
                .into_inner()
                .template
                .ok_or_else(|| unavailable("was answered without its content".into())),
            Err(status) if status.code() == tonic::Code::NotFound => {
                Err(unavailable("does not exist".into()))
            }
            Err(status) => Err(unavailable(format!(
                "cannot be read: the gateway answered {}",
                status.code()
            ))),
        }
    }
    /// The version of `template`: its backend on the gateway's one driver and the
    /// provider profiles every sandbox of it attaches, checked now.
    async fn version(&self, template: p::SandboxWorkloadTemplate) -> Result<Provisioner> {
        let mut grpc = self.gateway.client.raw_grpc_fresh().await?;
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
        ensure!(
            template
                .spec
                .as_ref()
                .and_then(|v| v.workload.as_ref())
                .is_some_and(|v| !v.image.is_empty()),
            "Computer native image unspecified"
        );
        let backend = super::spec::persistence(&info.compute_drivers[0], &template)?;
        let spec = super::ComputerProvisionSpec::new(
            (*self.config).clone(),
            self.model_secret.clone(),
            self.hub_url.clone(),
            backend,
            &self.policy,
            &template,
        );
        spec.version()?;
        let provisioner = self.for_spec(spec)?;
        provisioner.validate_profiles().await?;
        Ok(provisioner)
    }
}
impl Provisioner {
    /// A provisioner of already admitted parts, such as a test's fake gateway.
    #[cfg(test)]
    pub(super) fn of(
        gateway: Gateway,
        config: Arc<Computers>,
        policy: p::sandbox::v1::SandboxPolicy,
        public: &str,
        spec: super::ComputerProvisionSpec,
    ) -> Self {
        Self {
            gateway,
            config,
            policy,
            public: public.into(),
            spec,
        }
    }
    async fn validate_profiles(&self) -> Result<()> {
        let public = &self.public;
        let mut grpc = self.gateway.client.raw_grpc_fresh().await?;
        for (id, key, refresh) in [
            (
                &self.config.hub_provider.profile,
                &self.config.hub_provider.credential_key,
                Some(Strategy::Oauth2RefreshToken),
            ),
            (
                &self.config.model_provider.profile,
                &self.config.model_provider.credential_key,
                None,
            ),
            (
                &self.config.lore_provider.profile,
                &self.config.lore_provider.credential_key,
                Some(Strategy::Oauth2ClientCredentials),
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
            // The Hub profile refreshes the user's grant and the Lore profile mints with
            // the Computer's own client, both at this Hub's token endpoint.
            if let Some(expected) = refresh {
                let strategy = credential
                    .refresh
                    .as_ref()
                    .context("Computer refreshing profile has no native refresh strategy")?;
                ensure!(
                    strategy.strategy == expected as i32
                        && strategy.scopes.is_empty()
                        && strategy.token_url
                            == format!("{}/oauth/token", public.trim_end_matches('/'))
                        && !credential.stable_placeholder,
                    "Computer refreshing profile cannot own this refresh grant"
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
    /// The owner's clone of this version's template. Its workload's environment adds
    /// the Computer's base path, the Hub MCP URL, the version and the owner's Lore
    /// identity, which Lore's command line requires with the Lore provider's
    /// placeholder, a token it cannot read; the token Hub issues names the same
    /// identity.
    fn template(&self, computer: &Computer) -> Result<p::SandboxWorkloadTemplate> {
        let name = computer.native_name();
        let version = self.spec.version()?;
        let mut template = self.spec.template()?;
        template.metadata = Some(metadata(&name, &name));
        let environment = &mut template
            .spec
            .as_mut()
            .context("native template has no spec")?
            .workload
            .as_mut()
            .context("native template has no workload")?
            .environment;
        environment.insert("PUBLIC_BASE_PATH".into(), computer.base_path());
        environment.insert("COMPUTER_HUB_URL".into(), self.spec.hub_url.clone());
        environment.insert("LORE_IDENTITY".into(), computer.principal.to_string());
        environment.insert(COMPUTER_VERSION.into(), version.to_string());
        Ok(template)
    }
    /// Runs only while the caller owns the durable handoff row lock. A bounded
    /// cold-path transaction serializes custody, not an exactly-once claim across
    /// crashes between provider consumption and native durable publication. Custody
    /// only: the owner's workspace, its providers and their refresh, rotated now. A
    /// running sandbox keeps running and receives the rotated credentials behind its
    /// placeholders; entry creates a missing one.
    pub async fn handoff(
        &self,
        _catalog: &HubCatalog,
        computer: &Computer,
        grant: LoginGrant,
    ) -> Result<()> {
        // Profile definitions can be updated by native operators. Recheck their
        // actual recipient/strategy before transferring a new user grant.
        self.validate_profiles().await?;
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
        let model = HubSecretResolver::new()
            .resolve_reference(&self.spec.model_secret, SecretPurpose::ProviderApiKey)
            .await?;
        self.ensure_provider(
            computer,
            ProviderSeed {
                name: MODEL_PROVIDER,
                profile: &self.config.model_provider.profile,
                key: &self.config.model_provider.credential_key,
                value: Some(model.expose_secret()),
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
                value: Some(grant.access.expose_secret()),
                expiry: Some(grant.access_expiry),
                request_id: computer.hub_provider_request,
            },
        )
        .await?;
        self.ensure_provider(
            computer,
            ProviderSeed {
                name: LORE_PROVIDER,
                profile: &self.config.lore_provider.profile,
                key: &self.config.lore_provider.credential_key,
                value: None,
                expiry: None,
                request_id: Uuid::new_v5(&handoff, b"lore-provider"),
            },
        )
        .await?;
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
        rotate(
            &self.gateway.client,
            &name,
            HUB_PROVIDER,
            &self.config.hub_provider.credential_key,
            computer
                .rotate_request
                .context("Computer rotate ID missing")?
                .to_string(),
            "rotate user credential",
        )
        .await?;
        self.lore_refresh(&name, handoff, &grant.lore).await?;
        drop(grant);
        Ok(())
    }
    /// Delete the Computer's sandbox, keeping its workspace volume, which the next
    /// sandbox of its name adopts, and wait until it is gone. `operation` derives the
    /// delete's request ID.
    pub async fn remove(&self, computer: &Computer, operation: Uuid) -> Result<()> {
        let name = computer.native_name();
        let Some(sandbox) = self.sandbox(&name).await? else {
            return Ok(());
        };
        if phase(&sandbox) != p::SandboxPhase::Deleting {
            let mut grpc = self.gateway.client.raw_grpc_fresh().await?;
            let answer = tokio::time::timeout(
                DELETE_ANSWER,
                grpc.delete_sandbox(p::DeleteSandboxRequest {
                    workspace_scope: scope(&name),
                    name: name.clone(),
                    allow_missing: true,
                    keep_workspace: true,
                    request_id: request(operation, "delete"),
                }),
            )
            .await;
            match answer {
                Ok(answer) => {
                    answer
                        .map_err(|e| native_error("delete sandbox", e))
                        .context("deleting the sandbox")?;
                }
                Err(_) => tracing::info!(
                    sandbox = %name,
                    "the gateway has not answered the delete within {DELETE_ANSWER:?}; waiting for the sandbox to go"
                ),
            }
        }
        self.wait_gone(&name).await
    }
    /// Replace the Computer's sandbox with one of this version: delete the sandbox,
    /// keeping its workspace volume, which the next sandbox of its name adopts; clone
    /// this version's template for the owner; create the sandbox from it and wait until
    /// its listener answers. `operation` derives every request ID, so a retried request
    /// of the same operation is the same native mutation.
    pub async fn replace(&self, computer: &Computer, operation: Uuid) -> Result<()> {
        self.remove(computer, operation).await?;
        let name = computer.native_name();
        let mut grpc = self.gateway.client.raw_grpc_fresh().await?;
        tokio::time::timeout(
            RPC_TIMEOUT,
            grpc.delete_sandbox_template(p::DeleteSandboxTemplateRequest {
                workspace_scope: scope(&name),
                name: name.clone(),
                allow_missing: true,
                request_id: request(operation, "delete-template"),
            }),
        )
        .await
        .context("replacing the template clone: the gateway did not answer")?
        .map_err(|e| native_error("delete cloned template", e))
        .context("replacing the template clone")?;
        tokio::time::timeout(
            RPC_TIMEOUT,
            grpc.create_sandbox_template(p::CreateSandboxTemplateRequest {
                workspace_scope: scope(&name),
                template: Some(self.template(computer)?),
                request_id: request(operation, "template"),
            }),
        )
        .await
        .context("cloning the template: the gateway did not answer")?
        .map_err(|e| native_error("clone template", e))
        .context("cloning the template")?;
        tokio::time::timeout(
            RPC_TIMEOUT,
            grpc.create_sandbox(self.create_request(computer, operation)),
        )
        .await
        .context("creating the sandbox: the gateway did not answer")?
        .map_err(create_refused)
        .context("creating the sandbox")?;
        self.wait_phase(&name, p::SandboxPhase::Ready)
            .await
            .context("starting the sandbox")?;
        self.wait_listener(&name)
            .await
            .context("waiting for the Computer to listen")
    }
    /// The create request of the owner's sandbox, from its cloned template, with this
    /// version's policy and command and every Computer provider attached.
    fn create_request(&self, computer: &Computer, operation: Uuid) -> p::CreateSandboxRequest {
        let name = computer.native_name();
        p::CreateSandboxRequest {
            workspace_scope: scope(&name),
            name: name.clone(),
            workload_template: name,
            request_id: request(operation, "create"),
            annotations: HashMap::from([(RETENTION.into(), "persistent".into())]),
            spec: Some(p::SandboxSpec {
                policy: Some(self.policy.clone()),
                command: self.config.command.clone(),
                providers: PROVIDERS.map(String::from).to_vec(),
                restart_policy: p::SandboxRestartPolicy::Always as i32,
                ..Default::default()
            }),
            ..Default::default()
        }
    }
    /// Give the gateway the Computer's own Lore client, with its new secret, as the
    /// refresh of the Lore provider, and refresh the token now: the Hub issues the owner's
    /// login token, or a repository's token for a scope that names it. The sandbox then has
    /// Lore access from a client the Hub accepts.
    async fn lore_refresh(&self, name: &str, handoff: Uuid, client: &CustodyClient) -> Result<()> {
        let key = &self.config.lore_provider.credential_key;
        let mut grpc = self.gateway.client.raw_grpc_fresh().await?;
        tokio::time::timeout(
            RPC_TIMEOUT,
            grpc.configure_provider_refresh(p::ConfigureProviderRefreshRequest {
                request_id: request(handoff, "configure-lore"),
                ..client.refresh(scope(name), LORE_PROVIDER.into(), key.clone())
            }),
        )
        .await?
        .map_err(|e| native_error("configure Lore refresh custody", e))?;
        rotate(
            &self.gateway.client,
            name,
            LORE_PROVIDER,
            key,
            request(handoff, "rotate-lore"),
            "rotate Lore credential",
        )
        .await
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
                        // The gateway writes a minted credential at its first rotation.
                        && (value.is_none() || provider.credentials.contains_key(key)),
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
                            credentials: value
                                .map(|value| HashMap::from([(env.clone(), value.into())]))
                                .unwrap_or_default(),
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
    /// Whether the Hub can still serve `sandbox` as this Computer at this version: its
    /// name, retention, template clone, command, policy, providers, restart policy and
    /// base path are the ones this version gives.
    pub(super) fn validate_sandbox(&self, computer: &Computer, sandbox: &p::Sandbox) -> Result<()> {
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
                && spec.providers == PROVIDERS
                && spec.restart_policy == p::SandboxRestartPolicy::Always as i32
                && spec.environment.get("PUBLIC_BASE_PATH") == Some(&computer.base_path()),
            "its sandbox differs from what its version gives: its name, retention, template, command, policy, providers, restart policy or base path changed"
        );
        Ok(())
    }
    pub(super) async fn sandbox(&self, name: &str) -> Result<Option<p::Sandbox>> {
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
    /// Wait until the sandbox is gone, for at most [`DELETE_TIMEOUT`]; past it, say what
    /// still runs.
    async fn wait_gone(&self, name: &str) -> Result<()> {
        let deadline = Instant::now() + DELETE_TIMEOUT;
        loop {
            let Some(sandbox) = self.sandbox(name).await? else {
                return Ok(());
            };
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "the old sandbox was not gone after {} s: {}",
                    DELETE_TIMEOUT.as_secs(),
                    still_running(&sandbox)
                );
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    pub(super) async fn wait_phase(&self, name: &str, expected: p::SandboxPhase) -> Result<()> {
        tokio::time::timeout(READY_TIMEOUT, async {
            loop {
                let sandbox = self
                    .sandbox(name)
                    .await?
                    .context("the sandbox disappeared while starting")?;
                let observed = phase(&sandbox);
                if observed == expected {
                    return Ok(());
                }
                ensure!(
                    observed != p::SandboxPhase::Deleting && observed != p::SandboxPhase::Error,
                    "the sandbox entered phase {}",
                    phase_name(observed)
                );
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
        .await
        .with_context(|| {
            format!(
                "the sandbox was not {} after {} s",
                phase_name(expected),
                READY_TIMEOUT.as_secs()
            )
        })?
    }
}
/// How Computers' availability changed between two reads of the template.
#[derive(Debug, PartialEq, Eq)]
enum Availability<'a> {
    Unchanged,
    /// They became unavailable, or the reason changed.
    Unavailable(&'a str),
    /// They are available again after being unavailable.
    Restored,
}
fn availability<'a>(
    previous: Option<&Result<Provisioner, Unavailable>>,
    next: &'a Result<Provisioner, Unavailable>,
) -> Availability<'a> {
    match (previous, next) {
        (Some(Err(before)), Err(now)) if before.0 == now.0 => Availability::Unchanged,
        (_, Err(now)) => Availability::Unavailable(&now.0),
        (Some(Err(_)), Ok(_)) => Availability::Restored,
        (_, Ok(_)) => Availability::Unchanged,
    }
}
/// Log a change of Computers' availability once, not on every request that sees it.
fn log_change(
    previous: Option<&Result<Provisioner, Unavailable>>,
    next: &Result<Provisioner, Unavailable>,
) {
    match availability(previous, next) {
        Availability::Unchanged => {}
        Availability::Unavailable(cause) => {
            tracing::warn!(
                cause,
                "Computers are unavailable: no new Computer sandbox can start"
            );
        }
        Availability::Restored => {
            let version = next
                .as_ref()
                .ok()
                .and_then(|current| current.spec.version().ok())
                .map(|version| version.to_string())
                .unwrap_or_default();
            tracing::info!(%version, "Computers are available again");
        }
    }
}

/// Refresh a provider's credential now and require a current, unfailed result.
async fn rotate(
    client: &openshell_sdk::OpenShellClient,
    name: &str,
    provider: &str,
    key: &str,
    request_id: String,
    operation: &'static str,
) -> Result<()> {
    let mut grpc = client.raw_grpc_fresh().await?;
    let rotated = tokio::time::timeout(
        RPC_TIMEOUT,
        grpc.rotate_provider_credential(p::RotateProviderCredentialRequest {
            workspace_scope: scope(name),
            provider: provider.into(),
            credential_key: key.into(),
            request_id,
        }),
    )
    .await?
    .map_err(|e| native_error(operation, e))?
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
    Ok(())
}
pub(super) fn phase(sandbox: &p::Sandbox) -> p::SandboxPhase {
    p::SandboxPhase::try_from(sandbox.status.as_ref().map_or(0, |status| status.phase))
        .unwrap_or(p::SandboxPhase::Unknown)
}
/// What a sandbox that has not gone still holds: its phase, its Pod and the conditions
/// the gateway last observed.
fn still_running(sandbox: &p::Sandbox) -> String {
    let status = sandbox.status.clone().unwrap_or_default();
    let mut text = format!("it is still {}", phase_name(phase(sandbox)));
    if !status.agent_pod.is_empty() {
        text.push_str(&format!(" with Pod {}", status.agent_pod));
    }
    for condition in &status.conditions {
        text.push_str(&format!("; {} {}", condition.r#type, condition.status));
        if !condition.reason.is_empty() {
            text.push_str(&format!(" ({})", condition.reason));
        }
        if !condition.message.is_empty() {
            text.push_str(&format!(": {}", condition.message));
        }
    }
    text
}
/// A phase's name, as status reports it.
pub(super) fn phase_name(phase: p::SandboxPhase) -> &'static str {
    match phase {
        p::SandboxPhase::Unspecified | p::SandboxPhase::Unknown => "unknown",
        p::SandboxPhase::Provisioning => "provisioning",
        p::SandboxPhase::Ready => "ready",
        p::SandboxPhase::Error => "error",
        p::SandboxPhase::Deleting => "deleting",
        p::SandboxPhase::Stopping => "stopping",
        p::SandboxPhase::Stopped => "stopped",
        p::SandboxPhase::Starting => "starting",
        p::SandboxPhase::Completed => "completed",
    }
}

#[cfg(test)]
mod tests;
