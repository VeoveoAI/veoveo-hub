//! Admission of RFC 7591 registrations and RFC 7592 reads, updates and removals, in
//! the order the registration protocol prescribes.

use std::{sync::Arc, time::Duration};

use crate::audit::{Registration, RegistrationActivity, hub_audit};
use crate::contract::SecretPurpose;
use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use veoveo_audit_contract::{AuditDraft, AuditOutcome, AuditRequest};
use veoveo_hub_sdk::registration::{
    AccessTokenTrust, ClientInformation, ClientMetadata, ClientRegistrationRequest,
    ClientUpdateRequest, DatabaseGrant, DatabaseIdentifier, DatabaseUrl, Ed25519Jwk, Ed25519JwkSet,
    RegistrationAccessToken, RegistrationErrorCode, ResourceIndicator, SoftwareStatement,
    StorageBinding, statement::verify_statement,
};
use veoveo_hub_sdk::{HubInternalSigningKey, TokenIssuer};
use veoveo_store::{LoginName, ServerSchema};
use veoveo_types::{OAuthClientId, Sha256Digest};

use super::{
    RegistrationError,
    store::{EndedRegistration, NewRegistration, Pin, RegistrationStore, StoredRegistration},
    watch::LiveRegistrations,
};
use crate::{HubCatalogHandle, HubSecretResolver, HubState};

/// The database a registered server reaches, as its registration grant names it.
/// Servers and the Hub may reach the same database at different addresses.
#[derive(Debug, Clone)]
pub struct ServerDatabase {
    pub url: DatabaseUrl,
}

/// The path of the Hub issuer's JWK Set, below the issuer's origin.
pub(super) const JWKS_PATH: &str = "/jwks.json";

/// The Hub's identity as an authorization server.
#[derive(Debug, Clone)]
pub struct RegistrationIdentity {
    /// The issuer URL the RFC 8414 metadata announces and forwarded tokens carry.
    pub issuer: TokenIssuer,
    /// The key that signs forwarded access tokens.
    pub signing_key: HubInternalSigningKey,
    /// A public key published beside the signing key's and never used to sign: the
    /// next key before a rotation, or the previous one until its tokens expire.
    pub verification_key: Option<Ed25519Jwk>,
}

impl RegistrationIdentity {
    /// The keys servers verify forwarded tokens with: the signing key's public half
    /// first, then the verification key. Each `kid` is an RFC 7638 thumbprint.
    pub fn access_token_keys(&self) -> anyhow::Result<Ed25519JwkSet> {
        let signing = self.signing_key.verification_key();
        let mut keys = vec![signing];
        if let Some(verification) = self.verification_key {
            anyhow::ensure!(
                verification.kid() != signing.kid(),
                "the forwarded token verification key must differ from the signing key"
            );
            keys.push(verification);
        }
        Ok(Ed25519JwkSet::new(keys)?)
    }
}

#[derive(Clone)]
pub struct RegistrationService {
    pub(super) inner: Arc<Inner>,
}

pub(super) struct Inner {
    pub(super) store: RegistrationStore,
    pub(super) catalog: HubCatalogHandle,
    pub(super) hub: HubState,
    pub(super) issuer: TokenIssuer,
    pub(super) registration_endpoint: url::Url,
    pub(super) jwks_uri: url::Url,
    pub(super) access_token_keys: Ed25519JwkSet,
    pub(super) database: ServerDatabase,
    /// How long a registration may go without a live kernel instance of its login.
    pub(super) grace: Duration,
    pub(super) live: parking_lot::RwLock<LiveRegistrations>,
}

/// A request's bearer credential, as the endpoint received it.
pub enum Bearer<'a> {
    Missing,
    Presented(&'a str),
}

impl RegistrationService {
    pub fn new(
        store: veoveo_store::PlatformStore,
        catalog: HubCatalogHandle,
        hub: HubState,
        identity: RegistrationIdentity,
        database: ServerDatabase,
        grace: Duration,
    ) -> anyhow::Result<Self> {
        veoveo_hub_sdk::registration::issuer_url(&identity.issuer)
            .context("the Hub issuer must be an https URL, or http on a local host")?;
        let issuer_url = url::Url::parse(identity.issuer.as_str())?;
        let mut registration_endpoint = issuer_url.clone();
        registration_endpoint.set_path("/oauth/register");
        let mut jwks_uri = issuer_url;
        jwks_uri.set_path(JWKS_PATH);
        let access_token_keys = identity.access_token_keys()?;
        Ok(Self {
            inner: Arc::new(Inner {
                store: RegistrationStore::new(store),
                catalog,
                hub,
                issuer: identity.issuer,
                registration_endpoint,
                jwks_uri,
                access_token_keys,
                database,
                grace,
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

    /// The token endpoint where registered servers exchange tokens (RFC 8693).
    pub fn token_endpoint(&self) -> url::Url {
        let mut endpoint = self.inner.registration_endpoint.clone();
        endpoint.set_path("/oauth/token");
        endpoint
    }

    /// The RFC 8414 `jwks_uri` of the Hub's issuer, which publishes
    /// [`Self::access_token_keys`].
    pub fn jwks_uri(&self) -> &url::Url {
        &self.inner.jwks_uri
    }

    /// The public keys that verify forwarded access tokens, which every registration
    /// response names in `veoveo_access_tokens`. Each `kid` is the key's RFC 7638
    /// thumbprint, and no private member is present.
    pub fn access_token_keys(&self) -> &Ed25519JwkSet {
        &self.inner.access_token_keys
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

    /// RFC 7591 §3: register a server presenting the installation's registration
    /// secret as its initial access token. The response grants a new PostgreSQL login
    /// in the server's group role and the server's logical schema.
    pub async fn register(
        &self,
        bearer: Bearer<'_>,
        body: &[u8],
    ) -> Result<ClientInformation, RegistrationError> {
        self.check_initial_access_token(bearer).await?;
        let request: ClientRegistrationRequest = decode(body)?;
        request.metadata.check().map_err(invalid_metadata)?;
        let now = Utc::now();
        let slug = request.metadata.slug().clone();
        if slug.as_str() == crate::store::SCHEMA {
            return Err(RegistrationError::rejected(
                RegistrationErrorCode::InvalidClientMetadata,
                "software_id `hub` names the Hub's own schema; choose another slug",
            ));
        }
        let schema = ServerSchema::new(&slug).map_err(|error| {
            RegistrationError::rejected(
                RegistrationErrorCode::InvalidClientMetadata,
                format!("software_id cannot name a database schema: {error}"),
            )
        })?;
        let claim = self.inner.store.claim(&slug).await?;
        verify(
            &request.software_statement,
            &request.metadata,
            claim.as_deref(),
            &self.inner.registration_endpoint,
            now,
        )?;
        let client_id = new_client_id(&slug)?;
        let login = registration_login(&schema, &client_id)?;
        let password = random_token()?;
        let token = random_token()?;
        let pin = self
            .inner
            .store
            .register(NewRegistration {
                client_id: &client_id,
                metadata: &request.metadata,
                software_statement: &request.software_statement,
                token_digest: &digest(token.expose_secret()),
                login: &login,
                password: &password,
                now,
            })
            .await?;
        match pin {
            Pin::Foreign => {
                return Err(RegistrationError::rejected(
                    RegistrationErrorCode::UnapprovedSoftwareStatement,
                    "the namespace is pinned to another key",
                ));
            }
            Pin::Claimed => {
                self.audit(&slug, RegistrationActivity::ClaimNamespace)
                    .await?;
            }
            Pin::Held => {}
            Pin::Rebound => {
                return Err(RegistrationError::rejected(
                    RegistrationErrorCode::InvalidClientMetadata,
                    "a deployment cannot change its admitted identity or contract",
                ));
            }
        }
        self.audit(&slug, RegistrationActivity::Register).await?;
        self.reload(&client_id).await?;
        tracing::info!(server = %slug, client_id = %client_id, %login, "server registered");
        let database = DatabaseGrant {
            binding: StorageBinding::SharedLogicalSchema {
                server: slug.clone(),
            },
            url: self.inner.database.url.clone(),
            username: DatabaseIdentifier::new(login.as_str())
                .context("the login is a PostgreSQL role name")?,
            password,
        };
        self.information(
            client_id,
            now,
            RegistrationAccessToken::new(token.expose_secret()),
            request.metadata,
            request.software_statement,
            Some(database),
        )
    }

    /// RFC 7592 §2.1: read a registration with its registration access token.
    pub async fn read(
        &self,
        client_id: &str,
        bearer: Bearer<'_>,
    ) -> Result<ClientInformation, RegistrationError> {
        let (registration, token) = self.authenticate(client_id, bearer).await?;
        self.information(
            registration.client_id,
            registration.issued_at,
            RegistrationAccessToken::new(token),
            registration.metadata,
            registration.software_statement,
            None,
        )
    }

    /// RFC 7592 §2.2: replace the registered metadata. Changed declarations take the
    /// admission of a new registration under the pinned key; an unchanged update
    /// writes nothing.
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
        if request.metadata != registration.metadata {
            return Err(RegistrationError::rejected(
                RegistrationErrorCode::InvalidClientMetadata,
                "a deployment's key, identity, endpoint and contract are immutable; admit a new deployment",
            ));
        }
        let now = Utc::now();
        let audience = self.registration_client_uri(&registration.client_id);
        let claim = registration.metadata.key().thumbprint();
        verify(
            &request.software_statement,
            &request.metadata,
            Some(claim.as_str()),
            &audience,
            now,
        )?;
        self.information(
            registration.client_id,
            registration.issued_at,
            RegistrationAccessToken::new(token),
            request.metadata,
            request.software_statement,
            None,
        )
    }

    /// RFC 7592 §2.3: remove a registration and drop its login.
    pub async fn delete(
        &self,
        client_id: &str,
        bearer: Bearer<'_>,
    ) -> Result<(), RegistrationError> {
        let (registration, _) = self.authenticate(client_id, bearer).await?;
        if let Some(ended) = self.inner.store.deregister(&registration.client_id).await? {
            self.ended(&ended, RegistrationActivity::Deregister).await?;
        }
        self.reload(&registration.client_id).await?;
        Ok(())
    }

    /// Record why a registration ended.
    pub(super) async fn ended(
        &self,
        registration: &EndedRegistration,
        activity: RegistrationActivity,
    ) -> anyhow::Result<()> {
        self.audit(&registration.server, activity).await?;
        tracing::info!(
            server = %registration.server,
            client_id = %registration.client_id,
            login = %registration.login,
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
        let secret = HubSecretResolver::new()
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
    /// while the registration exists.
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
        if registration.token_digest != digest(presented) {
            return Err(unauthorized);
        }
        Ok((registration, presented.to_owned()))
    }

    fn information(
        &self,
        client_id: OAuthClientId,
        issued_at: DateTime<Utc>,
        token: RegistrationAccessToken,
        metadata: ClientMetadata,
        software_statement: SoftwareStatement,
        database: Option<DatabaseGrant>,
    ) -> Result<ClientInformation, RegistrationError> {
        let information = ClientInformation {
            registration_client_uri: self.registration_client_uri(&client_id),
            client_id,
            client_id_issued_at: issued_at,
            registration_access_token: token,
            access_tokens: AccessTokenTrust {
                deployment: metadata.registration.deployment,
                issuer: self.inner.issuer.clone(),
                jwks: self.inner.access_token_keys.clone(),
                resource: ResourceIndicator::from(&metadata.registration.endpoint),
            },
            metadata,
            software_statement,
            database,
        };
        information
            .check()
            .context("the Hub built an invalid registration response")?;
        Ok(information)
    }

    pub(super) async fn audit(
        &self,
        server: &veoveo_types::ServerSlug,
        activity: RegistrationActivity,
    ) -> anyhow::Result<()> {
        let draft = AuditDraft::builder(
            AuditRequest::background(),
            hub_audit().detail(&Registration { activity })?,
            AuditOutcome::Success,
        )
        .target(crate::audit::server_target(server)?)
        .build()?;
        self.inner.hub.record_audit(draft).await
    }
}

fn verify(
    statement: &SoftwareStatement,
    metadata: &ClientMetadata,
    pinned: Option<&str>,
    audience: &url::Url,
    now: DateTime<Utc>,
) -> Result<(), RegistrationError> {
    if let Some(pinned) = pinned
        && pinned != metadata.key().thumbprint().as_str()
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

fn invalid_metadata(error: veoveo_hub_sdk::registration::DeclarationError) -> RegistrationError {
    RegistrationError::rejected(
        RegistrationErrorCode::InvalidClientMetadata,
        error.to_string(),
    )
}

fn new_client_id(slug: &veoveo_types::ServerSlug) -> anyhow::Result<OAuthClientId> {
    OAuthClientId::parse(format!("{slug}-{}", uuid::Uuid::now_v7().simple()))
        .context("derive the registration's client identifier")
}

/// Hexadecimal digits of the client identifier's random tail that name its login. With
/// the longest schema name they fill PostgreSQL's 63-byte identifier limit.
const LOGIN_SUFFIX_DIGITS: usize = 14;

/// The registration's login, `<schema>_<suffix>`, where the suffix ends the client
/// identifier.
fn registration_login(
    schema: &ServerSchema,
    client_id: &OAuthClientId,
) -> anyhow::Result<LoginName> {
    let client = client_id.as_str();
    let suffix = client
        .get(client.len().saturating_sub(LOGIN_SUFFIX_DIGITS)..)
        .context("the client identifier is shorter than a login suffix")?;
    schema
        .login(suffix)
        .context("derive the registration's login name")
}

/// 256 random bits, base64url without padding.
fn random_token() -> anyhow::Result<SecretString> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| anyhow::anyhow!("random source: {error}"))?;
    Ok(SecretString::from(URL_SAFE_NO_PAD.encode(bytes)))
}

pub(super) fn digest(value: &str) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(value.as_bytes()).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logins_end_with_the_client_identifier_and_fit_postgres_names() {
        let longest = veoveo_types::ServerSlug::parse("a".repeat(48)).unwrap();
        let schema = ServerSchema::new(&longest).unwrap();
        let client_id = new_client_id(&longest).unwrap();
        let login = registration_login(&schema, &client_id).unwrap();
        assert_eq!(login.as_str().len(), 63);
        assert!(client_id.as_str().ends_with(&login.as_str()[49..]));
        let short = veoveo_types::ServerSlug::parse("time-zones").unwrap();
        let client_id = new_client_id(&short).unwrap();
        let login = registration_login(&ServerSchema::new(&short).unwrap(), &client_id).unwrap();
        assert!(login.as_str().starts_with("time_zones_"));
        assert!(DatabaseIdentifier::new(login.as_str()).is_ok());
    }

    #[test]
    fn a_verification_key_is_published_after_the_signing_key_and_never_equals_it() {
        use veoveo_hub_sdk::registration::statement::ServerKeyPair;
        let signing_pair = ServerKeyPair::generate().unwrap();
        let signing_key = HubInternalSigningKey::new(signing_pair.pkcs8_der()).unwrap();
        let next = Ed25519Jwk::new(*ServerKeyPair::generate().unwrap().public_key());
        let identity = |verification_key| RegistrationIdentity {
            issuer: TokenIssuer::parse("https://hub.example").unwrap(),
            signing_key: signing_key.clone(),
            verification_key,
        };
        let alone = serde_json::to_value(identity(None).access_token_keys().unwrap()).unwrap();
        assert_eq!(alone["keys"].as_array().unwrap().len(), 1);
        assert_eq!(alone["keys"][0]["kid"], signing_key.key_id());
        let both = serde_json::to_value(identity(Some(next)).access_token_keys().unwrap()).unwrap();
        assert_eq!(
            both["keys"],
            serde_json::json!([signing_key.verification_key(), next])
        );
        assert!(both["keys"][1].get("d").is_none());
        assert!(
            identity(Some(signing_key.verification_key()))
                .access_token_keys()
                .is_err()
        );
    }

    #[test]
    fn access_token_digests_name_their_algorithm() {
        let value = digest("registration-access-token");
        assert!(value.as_str().starts_with("sha256:"));
        assert_eq!(value, digest("registration-access-token"));
        assert_ne!(value, digest("another-token"));
    }
}
