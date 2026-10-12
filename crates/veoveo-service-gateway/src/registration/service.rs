//! Admission of RFC 7591 registrations and RFC 7592 reads, updates and removals, in
//! the order the registration protocol prescribes.

use std::{collections::BTreeMap, sync::Arc};

use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, TimeDelta, Utc};
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use veoveo_audit_contract::{
    AuditDetail, AuditDraft, AuditOutcome, AuditReason, AuditRequest, AuditTarget,
    RegistrationActivity,
};
use veoveo_gateway_contract::SecretPurpose;
use veoveo_mcp_contract::{GatewayInternalSigningKey, LeaseTimings, TokenIssuer};
use veoveo_mcp_ext_registration::{
    AccessTokenTrust, ClientInformation, ClientMetadata, ClientRegistrationRequest,
    ClientUpdateRequest, DatabaseGrant, DatabaseIdentifier, Ed25519JwkSet, RegistrationAccessToken,
    RegistrationErrorCode, RegistrationLease, ResourceIndicator, SoftwareStatement,
    statement::verify_statement,
};
use veoveo_modules::ModuleManifest;
use veoveo_types::{OAuthClientId, ServerSlug};

use super::{
    RegistrationError,
    modules::{ModuleHistory, admit_modules, apply_modules, compare_history},
    store::{NamespaceClaim, RegistrationStore, RegistrationWrite, StoredRegistration},
    watch::LiveRegistrations,
};
use crate::{GatewayCatalogHandle, GatewaySecretResolver, GatewayState};

/// Where registered servers reach the platform database. Servers and the Gateway may
/// reach the same database at different addresses.
#[derive(Debug, Clone)]
pub struct ServerDatabase {
    pub endpoint: url::Url,
    pub namespace: DatabaseIdentifier,
    pub database: DatabaseIdentifier,
}

/// The Gateway's identity as an authorization server.
#[derive(Debug, Clone)]
pub struct RegistrationIdentity {
    /// The issuer URL the RFC 8414 metadata announces and forwarded tokens carry.
    pub issuer: TokenIssuer,
    /// The key that signs forwarded access tokens.
    pub signing_key: GatewayInternalSigningKey,
}

#[derive(Clone)]
pub struct RegistrationService {
    pub(super) inner: Arc<Inner>,
}

pub(super) struct Inner {
    pub(super) store: RegistrationStore,
    pub(super) catalog: GatewayCatalogHandle,
    pub(super) gateway: GatewayState,
    pub(super) issuer: TokenIssuer,
    pub(super) registration_endpoint: url::Url,
    pub(super) access_token_keys: Ed25519JwkSet,
    pub(super) database: ServerDatabase,
    /// Serializes admissions on this replica, so migrations see each other's claims.
    pub(super) admission: tokio::sync::Mutex<()>,
    pub(super) live: parking_lot::RwLock<LiveRegistrations>,
}

/// A request's bearer credential, as the endpoint received it.
pub enum Bearer<'a> {
    Missing,
    Presented(&'a str),
}

impl RegistrationService {
    pub fn new(
        store: veoveo_platform_store::PlatformStore,
        catalog: GatewayCatalogHandle,
        gateway: GatewayState,
        identity: RegistrationIdentity,
        database: ServerDatabase,
    ) -> anyhow::Result<Self> {
        veoveo_mcp_ext_registration::issuer_url(&identity.issuer)
            .context("the Gateway issuer must be an https URL, or http on a local host")?;
        let issuer_url = url::Url::parse(identity.issuer.as_str())?;
        let mut registration_endpoint = issuer_url.clone();
        registration_endpoint.set_path("/oauth/register");
        Ok(Self {
            inner: Arc::new(Inner {
                store: RegistrationStore::new(store),
                catalog,
                gateway,
                issuer: identity.issuer,
                registration_endpoint,
                access_token_keys: Ed25519JwkSet::new(vec![
                    identity.signing_key.verification_key(),
                ])?,
                database,
                admission: tokio::sync::Mutex::new(()),
                live: parking_lot::RwLock::new(LiveRegistrations::default()),
            }),
        })
    }

    pub fn issuer(&self) -> &TokenIssuer {
        &self.inner.issuer
    }

    /// The RFC 7591 registration endpoint.
    pub fn registration_endpoint(&self) -> &url::Url {
        &self.inner.registration_endpoint
    }

    /// The RFC 7592 client configuration endpoint of one registration.
    pub fn registration_client_uri(&self, client_id: &OAuthClientId) -> url::Url {
        let mut uri = self.inner.registration_endpoint.clone();
        uri.path_segments_mut()
            .expect("the registration endpoint is a hierarchical URL")
            .push(client_id.as_str());
        uri
    }

    /// Whether the installation configures server registration.
    pub fn enabled(&self) -> bool {
        self.inner
            .catalog
            .current()
            .control_plane()
            .registration
            .is_some()
    }

    fn lease_timings(&self) -> LeaseTimings {
        self.inner
            .catalog
            .current()
            .control_plane()
            .registration
            .as_ref()
            .map(|settings| settings.lease)
            .unwrap_or_default()
    }

    /// RFC 7591 §3: register a server presenting the installation's registration
    /// secret as its initial access token.
    pub async fn register(
        &self,
        bearer: Bearer<'_>,
        body: &[u8],
    ) -> Result<ClientInformation, RegistrationError> {
        self.check_initial_access_token(bearer).await?;
        let request: ClientRegistrationRequest = decode(body)?;
        request.metadata.check().map_err(invalid_metadata)?;
        let _admission = self.inner.admission.lock().await;
        let now = Utc::now();
        let slug = request.metadata.slug().clone();
        let claims = self.claims().await?;
        let claim = claims.get(&slug);
        verify(
            &request.software_statement,
            &request.metadata,
            claim,
            &self.inner.registration_endpoint,
            now,
        )?;
        let plan = module_plan(&request.metadata, claim, &claims)?;
        self.pin(&request.metadata, claim, plan.record.as_ref(), now)
            .await?;
        self.migrate(&slug, &plan, &claims).await?;

        let client_id = new_client_id(&slug)?;
        let database = match plan.apply {
            Some(_) => Some(self.issue_database_user(&client_id, &slug).await?),
            None => None,
        };
        let token = random_token()?;
        let lease = self.lease_timings();
        let expires_at = lease_expiry(now, lease);
        self.inner
            .store
            .create_registration(RegistrationWrite {
                client_id: &client_id,
                metadata: &request.metadata,
                software_statement: &request.software_statement,
                token_digest: &digest(token.expose_secret()),
                database_username: database.as_ref().map(|grant| &grant.username),
                issued_at: now,
                now,
                lease_expires_at: expires_at,
            })
            .await?;
        self.audit(&slug, RegistrationActivity::Register).await?;
        self.refresh().await?;
        tracing::info!(server = %slug, client_id = %client_id, "server registered");
        self.information(
            client_id,
            now,
            RegistrationAccessToken::new(token.expose_secret()),
            request.metadata,
            request.software_statement,
            expires_at,
            lease,
            database,
        )
    }

    /// RFC 7592 §2.1: read a registration with its registration access token.
    pub async fn read(
        &self,
        client_id: &str,
        bearer: Bearer<'_>,
    ) -> Result<ClientInformation, RegistrationError> {
        let (registration, token) = self.authenticate(client_id, bearer).await?;
        let lease = self.lease_timings();
        self.information(
            registration.client_id,
            registration.issued_at,
            RegistrationAccessToken::new(token),
            registration.metadata,
            registration.software_statement,
            registration.lease_expires_at,
            lease,
            None,
        )
    }

    /// RFC 7592 §2.2: renew the lease, or admit changed declarations under the
    /// pinned key.
    pub async fn update(
        &self,
        client_id: &str,
        bearer: Bearer<'_>,
        body: &[u8],
    ) -> Result<ClientInformation, RegistrationError> {
        let (registration, token) = self.authenticate(client_id, bearer).await?;
        let request: ClientUpdateRequest = decode(body)?;
        if request.client_id != registration.client_id {
            return Err(RegistrationError::rejected(
                RegistrationErrorCode::InvalidClientMetadata,
                "client_id differs from the client configuration endpoint",
            ));
        }
        request.metadata.check().map_err(invalid_metadata)?;
        if request.metadata.slug() != registration.metadata.slug() {
            return Err(RegistrationError::rejected(
                RegistrationErrorCode::InvalidClientMetadata,
                "an update cannot change software_id",
            ));
        }
        let now = Utc::now();
        let slug = request.metadata.slug().clone();
        let audience = self.registration_client_uri(&registration.client_id);
        let changed = request.metadata != registration.metadata;
        let _admission = if changed {
            Some(self.inner.admission.lock().await)
        } else {
            None
        };
        let claims = self.claims().await?;
        let claim = claims.get(&slug);
        if claim.is_none() {
            return Err(RegistrationError::rejected(
                RegistrationErrorCode::UnapprovedSoftwareStatement,
                "the namespace has no pinned key",
            ));
        }
        verify(
            &request.software_statement,
            &request.metadata,
            claim,
            &audience,
            now,
        )?;
        let mut database_username = registration.database_username.clone();
        let mut database = None;
        if changed {
            let plan = module_plan(&request.metadata, claim, &claims)?;
            self.pin(&request.metadata, claim, plan.record.as_ref(), now)
                .await?;
            self.migrate(&slug, &plan, &claims).await?;
            if plan.apply.is_some() && database_username.is_none() {
                let grant = self
                    .issue_database_user(&registration.client_id, &slug)
                    .await?;
                database_username = Some(grant.username.clone());
                database = Some(grant);
            }
        }
        let lease = self.lease_timings();
        let expires_at = lease_expiry(now, lease);
        let updated = self
            .inner
            .store
            .update_registration(RegistrationWrite {
                client_id: &registration.client_id,
                metadata: &request.metadata,
                software_statement: &request.software_statement,
                token_digest: &registration.token_digest,
                database_username: database_username.as_ref(),
                issued_at: registration.issued_at,
                now,
                lease_expires_at: expires_at,
            })
            .await?;
        if !updated {
            return Err(RegistrationError::Unauthorized { token_sent: true });
        }
        if changed {
            self.audit(&slug, RegistrationActivity::Update).await?;
            self.refresh().await?;
        }
        self.information(
            registration.client_id,
            registration.issued_at,
            RegistrationAccessToken::new(token),
            request.metadata,
            request.software_statement,
            expires_at,
            lease,
            database,
        )
    }

    /// RFC 7592 §2.3: remove a registration.
    pub async fn delete(
        &self,
        client_id: &str,
        bearer: Bearer<'_>,
    ) -> Result<(), RegistrationError> {
        let (registration, _) = self.authenticate(client_id, bearer).await?;
        if let Some(removed) = self
            .inner
            .store
            .delete_registration(&registration.client_id)
            .await?
        {
            self.release(&removed, RegistrationActivity::Deregister)
                .await?;
        }
        self.refresh().await?;
        Ok(())
    }

    /// Remove the database user of a registration that ended and record why.
    pub(super) async fn release(
        &self,
        registration: &StoredRegistration,
        activity: RegistrationActivity,
    ) -> anyhow::Result<()> {
        if let Some(username) = &registration.database_username {
            self.inner.store.remove_user(username).await?;
        }
        self.audit(registration.metadata.slug(), activity).await?;
        tracing::info!(
            server = %registration.metadata.slug(),
            client_id = %registration.client_id,
            ?activity,
            "server registration ended"
        );
        Ok(())
    }

    async fn check_initial_access_token(
        &self,
        bearer: Bearer<'_>,
    ) -> Result<(), RegistrationError> {
        let Bearer::Presented(presented) = bearer else {
            return Err(RegistrationError::Unauthorized { token_sent: false });
        };
        let catalog = self.inner.catalog.current();
        let Some(settings) = catalog.control_plane().registration.as_ref() else {
            tracing::warn!(
                "server registration refused: the installation configures no registration secret"
            );
            return Err(RegistrationError::Unauthorized { token_sent: true });
        };
        let secret = GatewaySecretResolver::new()
            .resolve_string(
                &catalog,
                &settings.secret,
                SecretPurpose::RegistrationSecret,
            )
            .await
            .context("resolve the registration secret")?;
        if digest(presented) != digest(secret.expose_secret()) {
            return Err(RegistrationError::Unauthorized { token_sent: true });
        }
        Ok(())
    }

    /// RFC 7592 §3: the registration access token authorizes its own registration
    /// while the lease is current.
    async fn authenticate(
        &self,
        client_id: &str,
        bearer: Bearer<'_>,
    ) -> Result<(StoredRegistration, String), RegistrationError> {
        let Bearer::Presented(presented) = bearer else {
            return Err(RegistrationError::Unauthorized { token_sent: false });
        };
        let unauthorized = RegistrationError::Unauthorized { token_sent: true };
        let Ok(client_id) = OAuthClientId::parse(client_id) else {
            return Err(unauthorized);
        };
        let Some(registration) = self.inner.store.registration(&client_id).await? else {
            return Err(unauthorized);
        };
        if registration.token_digest != digest(presented)
            || registration.lease_expires_at <= Utc::now()
        {
            return Err(unauthorized);
        }
        Ok((registration, presented.to_owned()))
    }

    async fn claims(&self) -> Result<BTreeMap<ServerSlug, NamespaceClaim>, RegistrationError> {
        Ok(self
            .inner
            .store
            .claims()
            .await?
            .into_iter()
            .map(|claim| (claim.slug.clone(), claim))
            .collect())
    }

    /// Pin the namespace to the server's key on its first registration.
    async fn pin(
        &self,
        metadata: &ClientMetadata,
        claim: Option<&NamespaceClaim>,
        module: Option<&ModuleManifest>,
        now: DateTime<Utc>,
    ) -> Result<(), RegistrationError> {
        let slug = metadata.slug();
        if claim.is_some() {
            if let Some(module) = module {
                self.inner.store.record_module(slug, module, now).await?;
            }
            return Ok(());
        }
        let thumbprint = metadata.key().thumbprint();
        if self
            .inner
            .store
            .create_claim(slug, thumbprint.as_str(), module, now)
            .await?
        {
            self.audit(slug, RegistrationActivity::ClaimNamespace)
                .await?;
            return Ok(());
        }
        // Another replica claimed the namespace first.
        match self.inner.store.claim(slug).await? {
            Some(claim) if claim.key_thumbprint == thumbprint.as_str() => {
                if let Some(module) = module {
                    self.inner.store.record_module(slug, module, now).await?;
                }
                Ok(())
            }
            _ => Err(RegistrationError::rejected(
                RegistrationErrorCode::UnapprovedSoftwareStatement,
                "the namespace is pinned to another key",
            )),
        }
    }

    async fn migrate(
        &self,
        slug: &ServerSlug,
        plan: &ModulePlan,
        claims: &BTreeMap<ServerSlug, NamespaceClaim>,
    ) -> Result<(), RegistrationError> {
        let Some(module) = &plan.apply else {
            return Ok(());
        };
        let known = registry_modules(claims, slug, module);
        apply_modules(
            self.inner.store.platform().client(),
            known.iter(),
            vec![module.name.clone()],
        )
        .await
        .map_err(|error| {
            tracing::warn!(server = %slug, "module migration failed: {error:#}");
            RegistrationError::rejected(
                RegistrationErrorCode::InvalidClientMetadata,
                "veoveo_module migrations failed to apply",
            )
        })?;
        self.audit(slug, RegistrationActivity::Migrate).await?;
        Ok(())
    }

    async fn issue_database_user(
        &self,
        client_id: &OAuthClientId,
        slug: &ServerSlug,
    ) -> Result<DatabaseGrant, RegistrationError> {
        let suffix = client_id
            .as_str()
            .rsplit('-')
            .next()
            .unwrap_or_default()
            .to_owned();
        let username =
            DatabaseIdentifier::new(format!("{}_{suffix}", slug.as_str().replace('-', "_")))
                .context("derive the server's database user name")?;
        let password = random_token()?;
        self.inner
            .store
            .define_user(&username, password.expose_secret())
            .await?;
        Ok(DatabaseGrant {
            endpoint: self.inner.database.endpoint.clone(),
            namespace: self.inner.database.namespace.clone(),
            database: self.inner.database.database.clone(),
            username,
            password,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn information(
        &self,
        client_id: OAuthClientId,
        issued_at: DateTime<Utc>,
        token: RegistrationAccessToken,
        metadata: ClientMetadata,
        software_statement: SoftwareStatement,
        expires_at: DateTime<Utc>,
        lease: LeaseTimings,
        database: Option<DatabaseGrant>,
    ) -> Result<ClientInformation, RegistrationError> {
        let information = ClientInformation {
            registration_client_uri: self.registration_client_uri(&client_id),
            client_id,
            client_id_issued_at: issued_at,
            registration_access_token: token,
            access_tokens: AccessTokenTrust {
                issuer: self.inner.issuer.clone(),
                jwks: self.inner.access_token_keys.clone(),
                resource: ResourceIndicator::from(&metadata.registration.endpoint),
            },
            metadata,
            software_statement,
            lease: RegistrationLease {
                expires_at,
                renew_every_seconds: lease.renew_every_seconds,
            },
            database,
        };
        information
            .check()
            .context("the Gateway built an invalid registration response")?;
        Ok(information)
    }

    pub(super) async fn audit(
        &self,
        server: &ServerSlug,
        activity: RegistrationActivity,
    ) -> anyhow::Result<()> {
        let draft = AuditDraft::builder(
            AuditRequest::background(),
            AuditTarget::Server {
                server: server.clone(),
            },
            AuditDetail::Registration { activity },
            AuditOutcome::Succeeded,
            AuditReason::Accepted,
        )
        .build()?;
        self.inner.gateway.record_audit(draft).await
    }
}

/// The module work one admission performs.
struct ModulePlan {
    /// The manifest to record on the claim: a longer history than the one recorded.
    record: Option<ModuleManifest>,
    /// The manifest whose lane must be current before the server may route.
    apply: Option<ModuleManifest>,
}

fn module_plan(
    metadata: &ClientMetadata,
    claim: Option<&NamespaceClaim>,
    claims: &BTreeMap<ServerSlug, NamespaceClaim>,
) -> Result<ModulePlan, RegistrationError> {
    let Some(received) = metadata.module.as_ref().map(|module| module.manifest()) else {
        return Ok(ModulePlan {
            record: None,
            apply: None,
        });
    };
    let recorded = claim.and_then(|claim| claim.module.as_ref());
    let (record, apply) = match compare_history(recorded, received) {
        ModuleHistory::Extends => (Some(received.clone()), received.clone()),
        ModuleHistory::Contained => (None, recorded.expect("contained in a record").clone()),
        ModuleHistory::Diverges => {
            return Err(RegistrationError::rejected(
                RegistrationErrorCode::InvalidClientMetadata,
                "veoveo_module changes a migration this namespace already applied",
            ));
        }
    };
    let known = registry_modules(claims, metadata.slug(), &apply);
    admit_modules(known.iter()).map_err(|error| {
        RegistrationError::rejected(
            RegistrationErrorCode::InvalidClientMetadata,
            format!("veoveo_module is not admissible: {error}"),
        )
    })?;
    Ok(ModulePlan {
        record,
        apply: Some(apply),
    })
}

/// Every claimed module, with `slug`'s replaced by `module`.
fn registry_modules(
    claims: &BTreeMap<ServerSlug, NamespaceClaim>,
    slug: &ServerSlug,
    module: &ModuleManifest,
) -> Vec<ModuleManifest> {
    claims
        .values()
        .filter(|claim| &claim.slug != slug)
        .filter_map(|claim| claim.module.clone())
        .chain(std::iter::once(module.clone()))
        .collect()
}

fn verify(
    statement: &SoftwareStatement,
    metadata: &ClientMetadata,
    claim: Option<&NamespaceClaim>,
    audience: &url::Url,
    now: DateTime<Utc>,
) -> Result<(), RegistrationError> {
    if let Some(claim) = claim
        && claim.key_thumbprint != metadata.key().thumbprint().as_str()
    {
        return Err(RegistrationError::rejected(
            RegistrationErrorCode::UnapprovedSoftwareStatement,
            "the namespace is pinned to another key",
        ));
    }
    verify_statement(statement, metadata, None, audience, now)
        .map_err(|error| RegistrationError::rejected(error.error_code(), error.to_string()))
}

fn decode<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, RegistrationError> {
    serde_json::from_slice(body).map_err(|error| {
        RegistrationError::rejected(
            RegistrationErrorCode::InvalidClientMetadata,
            format!(
                "the request body is not a valid registration document (line {}, column {})",
                error.line(),
                error.column()
            ),
        )
    })
}

fn invalid_metadata(error: veoveo_mcp_ext_registration::DeclarationError) -> RegistrationError {
    RegistrationError::rejected(
        RegistrationErrorCode::InvalidClientMetadata,
        error.to_string(),
    )
}

fn lease_expiry(now: DateTime<Utc>, lease: LeaseTimings) -> DateTime<Utc> {
    now + TimeDelta::seconds(i64::from(lease.expires_after_seconds))
}

fn new_client_id(slug: &ServerSlug) -> anyhow::Result<OAuthClientId> {
    OAuthClientId::parse(format!("{slug}-{}", uuid::Uuid::now_v7().simple()))
        .context("derive the registration's client identifier")
}

/// 256 random bits, base64url without padding.
fn random_token() -> anyhow::Result<SecretString> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| anyhow::anyhow!("random source: {error}"))?;
    Ok(SecretString::from(URL_SAFE_NO_PAD.encode(bytes)))
}

pub(super) fn digest(value: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value.as_bytes())))
}
