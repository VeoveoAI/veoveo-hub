//! Calls between servers: the RFC 8693 token exchange a registered server performs to
//! call other servers for its caller, and the admission of the token it receives at a
//! profile. The protocol is the SDK's
//! [calls between servers](https://github.com/VeoveoAI/veoveo-sdk-rust/blob/8cc3235fe03a8463bef57c661ae0b8a1a99a2715/crates/veoveo-sdk-rust/src/registration/DESIGN.md#calls-between-servers).
use std::{collections::BTreeSet, sync::Arc};

use anyhow::Context;
use chrono::{DateTime, TimeDelta, Utc};
use serde::Deserialize;
use veoveo_audit_contract::{AuditOutcome, AuditReason, AuditTarget};
use veoveo_sdk_rust::{
    ActingServer, DelegatedCaller, ExchangeGrant, ExchangedTokenVerifier, GatewayInternalIdentity,
    GatewayInternalTokenIssuer, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    OpaqueTaskId, TokenAudience, TokenIssuer,
    calls::{
        ACCESS_TOKEN_TOKEN_TYPE, TASK_TOKEN_TYPE, TokenErrorCode, TokenErrorResponse,
        TokenExchangeResponse,
    },
    registration::{
        ClientMetadata, GrantType, PermissionsDetail, ResourceIndicator,
        assertion::{CLIENT_ASSERTION_TYPE, asserted_client, verify_client_assertion},
    },
};
use veoveo_types::{OAuthClientId, PermissionName, ServerSlug};

use crate::{
    AuthenticatedSubject, GatewayCatalog, GatewayCatalogHandle, GatewayState,
    audit::{ExchangeRefusal, ExchangeSubjectKind, TokenExchange, gateway_audit},
    contract::AuthorizationServerId,
    control_plane::GatewayProfile,
    registration::RegistrationService,
};

/// The longest an exchanged token lives when its subject is a Task.
pub const TASK_DELEGATION_LIFETIME: TimeDelta = TimeDelta::minutes(5);
/// The replay guard's authorization server for registered servers' assertions.
const REGISTERED_SERVERS: &str = "registered-servers";

/// A call another registered server made for its caller through the Gateway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectDelegation {
    pub acting_server: ActingServer,
    /// The permissions the exchange granted; the call carries no others.
    pub permissions: BTreeSet<PermissionName>,
    /// The calling server's Task the delegation lasts as long as, and which the call
    /// names as provenance.
    pub task: Option<OpaqueTaskId>,
}

impl SubjectDelegation {
    /// `granted` narrowed to what this delegation carries.
    pub fn narrow(&self, granted: BTreeSet<PermissionName>) -> BTreeSet<PermissionName> {
        granted
            .into_iter()
            .filter(|permission| self.permissions.contains(permission))
            .collect()
    }
}

/// One RFC 8693 token request as the token endpoint read it.
#[derive(Debug, Default, Deserialize)]
pub struct TokenExchangeRequest {
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_assertion_type: Option<String>,
    #[serde(default)]
    pub client_assertion: Option<String>,
    #[serde(default)]
    pub subject_token: Option<String>,
    #[serde(default)]
    pub subject_token_type: Option<String>,
    #[serde(default)]
    pub resource: Option<String>,
    #[serde(default)]
    pub authorization_details: Option<String>,
    #[serde(default)]
    pub requested_token_type: Option<String>,
    #[serde(default)]
    pub actor_token: Option<String>,
    #[serde(default)]
    pub audience: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

/// Why an exchange issued no token: an RFC 6749 §5.2 error.
#[derive(Debug)]
pub struct TokenExchangeRefusal {
    pub error: TokenErrorCode,
    pub description: String,
}

impl TokenExchangeRefusal {
    fn new(error: TokenErrorCode, description: impl Into<String>) -> Self {
        Self {
            error,
            description: description.into(),
        }
    }

    fn unavailable(error: anyhow::Error) -> Self {
        tracing::error!(error = %format!("{error:#}"), "token exchange state unavailable");
        Self::new(
            TokenErrorCode::ServerError,
            "the Gateway cannot judge the exchange now",
        )
    }

    /// The RFC 6749 §5.2 response document.
    pub fn response(&self) -> TokenErrorResponse {
        TokenErrorResponse {
            error: self.error,
            error_description: Some(self.description.clone()),
        }
    }

    fn audit_refusal(&self) -> Option<ExchangeRefusal> {
        Some(match self.error {
            TokenErrorCode::InvalidRequest => ExchangeRefusal::InvalidRequest,
            TokenErrorCode::UnauthorizedClient => ExchangeRefusal::UnauthorizedClient,
            TokenErrorCode::InvalidGrant => ExchangeRefusal::InvalidGrant,
            TokenErrorCode::InvalidTarget => ExchangeRefusal::InvalidTarget,
            TokenErrorCode::InvalidAuthorizationDetails => {
                ExchangeRefusal::InvalidAuthorizationDetails
            }
            TokenErrorCode::ServerError => ExchangeRefusal::ServerError,
            _ => return None,
        })
    }
}

/// A token presented at a profile that the Gateway issued in an exchange, and why it
/// was refused.
#[derive(Debug)]
pub enum ExchangedAdmission {
    /// Not an exchanged token: the profile's authorization servers judge it.
    NotExchanged,
    Admitted(Box<AuthenticatedSubject>),
    Refused(&'static str),
    Unavailable(anyhow::Error),
}

/// The exchange and the admission of its tokens.
#[derive(Clone)]
pub struct DelegationService {
    inner: Arc<Inner>,
}

struct Inner {
    registrations: RegistrationService,
    catalog: GatewayCatalogHandle,
    gateway: GatewayState,
    issuer: GatewayInternalTokenIssuer,
    issuer_id: TokenIssuer,
    trust: GatewayInternalTrustBundle,
    exchanged: ExchangedTokenVerifier,
    token_endpoint: url::Url,
}

/// An issued token, its subject and the number of permissions the server requested.
type Granted = (TokenExchangeResponse, Subject, u32);
/// A refusal, the subject when it was verified, and the number of requested permissions.
type Refused = Box<(TokenExchangeRefusal, Option<Subject>, u32)>;

/// A verified exchange subject: whom the token acts for, until when, and through which
/// profile.
struct Subject {
    kind: ExchangeSubjectKind,
    caller: DelegatedCaller,
    profile: veoveo_sdk_rust::GatewayProfileId,
    expires_at: DateTime<Utc>,
    task: Option<OpaqueTaskId>,
}

impl DelegationService {
    /// `issuer` signs forwarded and exchanged tokens with the key the registration
    /// identity publishes.
    pub fn new(
        registrations: RegistrationService,
        catalog: GatewayCatalogHandle,
        gateway: GatewayState,
        issuer: GatewayInternalTokenIssuer,
    ) -> anyhow::Result<Self> {
        let issuer_id = registrations.issuer().clone();
        let trust = GatewayInternalTrustBundle::from_key_set(registrations.access_token_keys())?;
        let token_endpoint = registrations.token_endpoint();
        Ok(Self {
            inner: Arc::new(Inner {
                exchanged: ExchangedTokenVerifier::new(issuer_id.clone(), trust.clone()),
                registrations,
                catalog,
                gateway,
                issuer,
                issuer_id,
                trust,
                token_endpoint,
            }),
        })
    }

    /// The token endpoint registered servers exchange tokens at.
    pub fn token_endpoint(&self) -> &url::Url {
        &self.inner.token_endpoint
    }

    /// RFC 8693 §2: exchange a subject for a token that calls other servers through
    /// one profile, carrying the requested uses the caller holds.
    pub async fn exchange(
        &self,
        request: TokenExchangeRequest,
    ) -> Result<TokenExchangeResponse, TokenExchangeRefusal> {
        use TokenErrorCode::*;
        if request.actor_token.is_some() || request.audience.is_some() || request.scope.is_some() {
            return Err(TokenExchangeRefusal::new(
                InvalidRequest,
                "actor_token, audience and scope are not part of this exchange; name the profile with resource and permissions with authorization_details",
            ));
        }
        if request
            .requested_token_type
            .as_deref()
            .is_some_and(|kind| kind != ACCESS_TOKEN_TOKEN_TYPE)
        {
            return Err(TokenExchangeRefusal::new(
                InvalidRequest,
                "the exchange issues access tokens only",
            ));
        }
        let (client, metadata) = self.authenticate(&request).await?;
        let slug = metadata.slug().clone();
        let outcome = self.grant(&request, &client, &metadata).await;
        self.audit(&slug, &outcome).await;
        outcome
            .map(|(response, _, _)| response)
            .map_err(|refused| refused.0)
    }

    /// RFC 7523 client authentication of a registered server with its pinned key.
    async fn authenticate(
        &self,
        request: &TokenExchangeRequest,
    ) -> Result<(OAuthClientId, ClientMetadata), TokenExchangeRefusal> {
        let invalid = || {
            TokenExchangeRefusal::new(
                TokenErrorCode::InvalidClient,
                "client authentication failed",
            )
        };
        let (Some(CLIENT_ASSERTION_TYPE), Some(assertion)) = (
            request.client_assertion_type.as_deref(),
            request.client_assertion.as_deref(),
        ) else {
            return Err(invalid());
        };
        let client = match request.client_id.as_deref().map(str::trim) {
            Some(named) if !named.is_empty() => OAuthClientId::parse(named).ok(),
            _ => asserted_client(assertion),
        }
        .ok_or_else(invalid)?;
        let metadata = self
            .inner
            .registrations
            .registered_client(&client)
            .ok_or_else(invalid)?;
        let verified = verify_client_assertion(
            assertion,
            &client,
            metadata.key(),
            &self.inner.token_endpoint,
            Utc::now(),
        )
        .map_err(|error| {
            tracing::warn!(%client, %error, "rejected a registered server's client assertion");
            invalid()
        })?;
        let fresh = self
            .inner
            .gateway
            .record_client_assertion_jti(
                &AuthorizationServerId::parse(REGISTERED_SERVERS).expect("a constant identifier"),
                &client,
                &verified.jwt_id,
                verified.expires_at,
                Utc::now(),
            )
            .await
            .map_err(TokenExchangeRefusal::unavailable)?;
        if !fresh {
            tracing::warn!(%client, "rejected a replayed client assertion");
            return Err(invalid());
        }
        if !metadata.grant_types.contains(&GrantType::TokenExchange) {
            return Err(TokenExchangeRefusal::new(
                TokenErrorCode::UnauthorizedClient,
                "the registration declares no uses, so it holds no token exchange grant",
            ));
        }
        Ok((client, metadata))
    }

    async fn grant(
        &self,
        request: &TokenExchangeRequest,
        client: &OAuthClientId,
        metadata: &ClientMetadata,
    ) -> Result<Granted, Refused> {
        let subject = self
            .subject(request, metadata)
            .await
            .map_err(|refusal| Box::new((refusal, None, 0)))?;
        match self.issue(request, client, metadata, &subject) {
            Ok((response, requested)) => Ok((response, subject, requested)),
            Err((refusal, requested)) => Err(Box::new((refusal, Some(subject), requested))),
        }
    }

    /// Issue the token for a verified subject: the requested uses, checked against the
    /// declaration, the subject's profile and the registered providers, narrowed to what
    /// the caller holds. Both outcomes carry the number of requested permissions.
    fn issue(
        &self,
        request: &TokenExchangeRequest,
        client: &OAuthClientId,
        metadata: &ClientMetadata,
        subject: &Subject,
    ) -> Result<(TokenExchangeResponse, u32), (TokenExchangeRefusal, u32)> {
        use TokenErrorCode::*;
        let slug = metadata.slug();
        let catalog = self.inner.catalog.current();
        let uses: BTreeSet<&PermissionName> = metadata.authorization.uses.iter().collect();
        let requested: BTreeSet<PermissionName> = match request.authorization_details.as_deref() {
            None => uses
                .iter()
                .map(|permission| (*permission).clone())
                .collect(),
            Some(details) => match serde_json::from_str::<Vec<PermissionsDetail>>(details) {
                Ok(details) if !details.is_empty() => details
                    .into_iter()
                    .flat_map(|detail| detail.actions)
                    .collect(),
                _ => {
                    return Err((
                        TokenExchangeRefusal::new(
                            InvalidAuthorizationDetails,
                            "authorization_details must hold permissions details",
                        ),
                        0,
                    ));
                }
            },
        };
        let count = u32::try_from(requested.len()).unwrap_or(u32::MAX);
        let refuse =
            |error, description: String| (TokenExchangeRefusal::new(error, description), count);
        if let Some(undeclared) = requested
            .iter()
            .find(|permission| !uses.contains(permission))
        {
            return Err(refuse(
                InvalidAuthorizationDetails,
                format!("{slug} declares no use of {undeclared}"),
            ));
        }
        let Some(profile) = catalog.profile(&subject.profile) else {
            return Err(refuse(
                InvalidTarget,
                "the subject's profile is gone".into(),
            ));
        };
        let resource = ResourceIndicator::parse(profile.protected_resource.as_str())
            .map_err(|_| refuse(InvalidTarget, "the profile's resource is no URL".into()))?;
        if request.resource.as_deref() != Some(resource.as_str()) {
            return Err(refuse(
                InvalidTarget,
                format!("resource must be {resource}, the profile the subject came through"),
            ));
        }
        let missing = missing_providers(&catalog, &requested);
        if !missing.is_empty() {
            return Err(refuse(
                InvalidTarget,
                format!("no registered server provides {}", missing.join(", ")),
            ));
        }
        let issued = self
            .inner
            .issuer
            .issue_exchanged(ExchangeGrant {
                profile: subject.profile.clone(),
                resource,
                caller: subject.caller.clone(),
                acting_server: ActingServer {
                    server: slug.clone(),
                    client: client.clone(),
                },
                permissions: self.held(&catalog, &requested, &subject.caller),
                task: subject.task.clone(),
                expires_at: subject.expires_at,
            })
            .map_err(|error| {
                refuse(
                    InvalidGrant,
                    format!("the subject cannot be delegated: {error}"),
                )
            })?;
        let expires_in = (issued.identity.expires_at - Utc::now())
            .num_seconds()
            .max(1) as u64;
        Ok((
            TokenExchangeResponse {
                access_token: issued.bearer_token,
                issued_token_type: ACCESS_TOKEN_TOKEN_TYPE.into(),
                token_type: "Bearer".into(),
                expires_in,
                authorization_details: (!issued.identity.permissions.is_empty())
                    .then(|| PermissionsDetail::new(issued.identity.permissions.clone()))
                    .into_iter()
                    .collect(),
            },
            count,
        ))
    }

    /// The requested permissions the caller holds now, under the installation's roles
    /// and each provider's current declaration.
    fn held(
        &self,
        catalog: &GatewayCatalog,
        requested: &BTreeSet<PermissionName>,
        caller: &DelegatedCaller,
    ) -> BTreeSet<PermissionName> {
        let installation = &catalog.control_plane().authorization;
        catalog
            .registrations()
            .iter()
            .flat_map(|server| {
                crate::policy::granted_permissions(
                    installation,
                    &server.authorization,
                    &caller.request_context.principal,
                )
            })
            .filter(|permission| requested.contains(permission))
            .collect()
    }

    /// The caller the subject acts for, and until when.
    async fn subject(
        &self,
        request: &TokenExchangeRequest,
        metadata: &ClientMetadata,
    ) -> Result<Subject, TokenExchangeRefusal> {
        use TokenErrorCode::*;
        let slug = metadata.slug();
        match (
            request.subject_token.as_deref(),
            request.subject_token_type.as_deref(),
        ) {
            (Some(token), Some(ACCESS_TOKEN_TOKEN_TYPE)) => {
                let identity = self.forwarded(token, metadata).ok_or_else(|| {
                    TokenExchangeRefusal::new(
                        InvalidGrant,
                        format!("the subject token is no live forwarded token of {slug}"),
                    )
                })?;
                let caller = DelegatedCaller::of(&identity).map_err(|error| match error {
                    veoveo_sdk_rust::InternalTokenError::RedelegatedIdentity => {
                        TokenExchangeRefusal::new(
                            InvalidGrant,
                            "a delegated identity cannot be delegated again",
                        )
                    }
                    _ => TokenExchangeRefusal::new(
                        InvalidGrant,
                        "the subject token carries no request context",
                    ),
                })?;
                Ok(Subject {
                    kind: ExchangeSubjectKind::Request,
                    caller,
                    profile: identity.profile,
                    expires_at: identity.expires_at,
                    task: None,
                })
            }
            (Some(task), Some(TASK_TOKEN_TYPE)) => {
                let task = OpaqueTaskId::parse(task).map_err(|_| {
                    TokenExchangeRefusal::new(InvalidGrant, "the subject is no Task identifier")
                })?;
                let delegation = self
                    .inner
                    .gateway
                    .task_delegation(slug, task.as_str())
                    .await
                    .map_err(TokenExchangeRefusal::unavailable)?
                    .ok_or_else(|| {
                        TokenExchangeRefusal::new(
                            InvalidGrant,
                            format!(
                                "{slug} has no Task {task} that a call through the Gateway created"
                            ),
                        )
                    })?;
                let live = self
                    .inner
                    .gateway
                    .task_live(slug, task.as_str(), &delegation.caller)
                    .await
                    .map_err(TokenExchangeRefusal::unavailable)?;
                if !live {
                    return Err(TokenExchangeRefusal::new(
                        InvalidGrant,
                        format!("Task {task} has ended"),
                    ));
                }
                Ok(Subject {
                    kind: ExchangeSubjectKind::Task,
                    caller: delegation.caller,
                    profile: delegation.profile,
                    expires_at: (Utc::now() + TASK_DELEGATION_LIFETIME).min(delegation.expires_at),
                    task: Some(task),
                })
            }
            (Some(_), Some(other)) => Err(TokenExchangeRefusal::new(
                InvalidRequest,
                format!("subject_token_type {other} is not supported"),
            )),
            _ => Err(TokenExchangeRefusal::new(
                InvalidRequest,
                "subject_token and subject_token_type are required",
            )),
        }
    }

    /// A forwarded token the Gateway addressed to the registered server `metadata`.
    fn forwarded(&self, token: &str, metadata: &ClientMetadata) -> Option<GatewayInternalIdentity> {
        GatewayInternalTokenVerifier::new(
            self.inner.issuer_id.clone(),
            TokenAudience {
                server: metadata.slug().clone(),
                resource: ResourceIndicator::from(&metadata.registration.endpoint),
            },
            self.inner.trust.clone(),
        )
        .verify(token)
        .ok()
    }

    async fn audit(&self, server: &ServerSlug, outcome: &Result<Granted, Refused>) {
        let (subject, detail, audit_outcome, reason) = match outcome {
            Ok((response, subject, requested)) => (
                Some(subject),
                TokenExchange {
                    subject: Some(subject.kind),
                    requested: *requested,
                    granted: u32::try_from(response.permissions().len()).unwrap_or(u32::MAX),
                    refusal: None,
                },
                AuditOutcome::Allowed,
                AuditReason::Accepted,
            ),
            Err(refused) => {
                let (refusal, subject, requested) = &**refused;
                (
                    subject.as_ref(),
                    TokenExchange {
                        subject: subject.as_ref().map(|subject| subject.kind),
                        requested: *requested,
                        granted: 0,
                        refusal: refusal.audit_refusal(),
                    },
                    AuditOutcome::Denied,
                    match refusal.error {
                        TokenErrorCode::ServerError => AuditReason::Unavailable,
                        TokenErrorCode::InvalidTarget => AuditReason::UnknownServer,
                        _ => AuditReason::PolicyDenied,
                    },
                )
            }
        };
        let draft = (|| {
            let detail = gateway_audit().detail(&detail)?;
            let target = AuditTarget::Server {
                server: server.clone(),
            };
            match subject {
                Some(subject) => subject_of(&subject.caller, None).audit_draft(
                    target,
                    detail,
                    audit_outcome,
                    reason,
                ),
                None => Ok(veoveo_audit_contract::AuditDraft::builder(
                    crate::request_observation::RequestObservation::current()
                        .map(|request| request.audit)
                        .unwrap_or_else(veoveo_audit_contract::AuditRequest::background),
                    target,
                    detail,
                    audit_outcome,
                    reason,
                )
                .build()?),
            }
        })();
        let recorded = match draft {
            Ok(draft) => self.inner.gateway.record_audit(draft).await,
            Err(error) => Err(error),
        };
        if let Err(error) = recorded {
            tracing::error!(error = %format!("{error:#}"), %server, "token exchange audit failed");
        }
    }

    /// Admit a bearer presented at `profile` when the Gateway issued it in an exchange.
    pub async fn admit(&self, token: &str, profile: &GatewayProfile) -> ExchangedAdmission {
        if !issued_by(token, &self.inner.issuer_id) {
            return ExchangedAdmission::NotExchanged;
        }
        let Ok(resource) = ResourceIndicator::parse(profile.protected_resource.as_str()) else {
            return ExchangedAdmission::Refused("the profile's resource is no URL");
        };
        let Ok(identity) = self.inner.exchanged.verify(token, &resource) else {
            return ExchangedAdmission::Refused("invalid exchanged token");
        };
        if identity.profile != profile.id {
            return ExchangedAdmission::Refused("the token is another profile's");
        }
        if let Some(task) = &identity.task {
            match self
                .inner
                .gateway
                .task_live(
                    &identity.acting_server.server,
                    task.as_str(),
                    &identity.caller,
                )
                .await
                .context("failed to read the delegating Task")
            {
                Ok(true) => {}
                Ok(false) => return ExchangedAdmission::Refused("the delegating Task has ended"),
                Err(error) => return ExchangedAdmission::Unavailable(error),
            }
        }
        ExchangedAdmission::Admitted(Box::new(subject_of(
            &identity.caller,
            Some(SubjectDelegation {
                acting_server: identity.acting_server,
                permissions: identity.permissions,
                task: identity.task,
            }),
        )))
    }
}

/// Whether a JWT names `issuer` as its `iss`, before any verification.
fn issued_by(token: &str, issuer: &TokenIssuer) -> bool {
    #[derive(Deserialize)]
    struct Issuer {
        iss: String,
    }
    jsonwebtoken::dangerous::insecure_decode::<Issuer>(token)
        .is_ok_and(|claims| claims.claims.iss == issuer.as_str())
}

/// The permissions no registered server declares.
fn missing_providers(
    catalog: &GatewayCatalog,
    requested: &BTreeSet<PermissionName>,
) -> Vec<String> {
    requested
        .iter()
        .filter(|permission| {
            !catalog
                .registrations()
                .iter()
                .any(|server| server.authorization.permission(permission).is_some())
        })
        .map(ToString::to_string)
        .collect()
}

/// The subject of a call another server makes for `caller`: the caller as the Gateway
/// authenticated it, in this request.
fn subject_of(
    caller: &DelegatedCaller,
    delegation: Option<SubjectDelegation>,
) -> AuthenticatedSubject {
    AuthenticatedSubject {
        extensions: Default::default(),
        audit: crate::request_observation::RequestObservation::current()
            .map(|request| request.audit)
            .unwrap_or_else(veoveo_audit_contract::AuditRequest::background),
        access_token: caller.request_context.access_token.clone(),
        principal: caller.request_context.principal.clone(),
        actor: caller.actor.clone(),
        principal_display_name: None,
        authority: caller.authority.clone(),
        delegation,
    }
}
