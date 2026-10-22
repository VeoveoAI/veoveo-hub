use std::{collections::HashMap, sync::Arc};

use axum::http::{HeaderName, HeaderValue};
use chrono::{TimeDelta, Utc};
use futures::stream::BoxStream;
use rmcp::{
    model::{ClientJsonRpcMessage, ClientRequest, ErrorData as McpError},
    transport::streamable_http_client::{
        AuthRequiredError, InsufficientScopeError, SseError, StreamableHttpClient,
        StreamableHttpError, StreamableHttpPostResponse,
    },
};
use sse_stream::Sse;
use std::collections::BTreeSet;
use thiserror::Error;
use veoveo_hub_sdk::registration::ResourceIndicator;
use veoveo_hub_sdk::{
    ForwardedRequest, HubInternalTokenIssuer, HubProfileId, HubRequestContext, InternalTokenError,
    Principal, ServerSlug,
};
use veoveo_types::{InvocationAuthority, OAuthClientId, PermissionName};

use super::upstream_http::UpstreamTransport;

const INTERNAL_REQUEST_TOKEN_TTL_SECONDS: i64 = 60;
const INTERNAL_SUBSCRIPTION_TOKEN_TTL_SECONDS: i64 = 15 * 60;

/// Per-request HTTP authorization for one auth-scoped hub-to-server client.
///
/// This client signs a lifetime-bounded assertion for every request while retaining
/// one immutable invocation authority. Final-profile POSTs and request-scoped
/// listener streams therefore never derive authority from connection locality.
#[derive(Clone)]
pub(super) struct HubAuthorizedHttpClient {
    http: UpstreamTransport,
    issuer: HubInternalTokenIssuer,
    profile: HubProfileId,
    server: ServerSlug,
    deployment: veoveo_hub_sdk::DeploymentId,
    audience: ResourceIndicator,
    client: OAuthClientId,
    actor: Principal,
    authority: InvocationAuthority,
    permissions: BTreeSet<PermissionName>,
    request_context: HubRequestContext,
    acting_server: Option<veoveo_hub_sdk::ActingServer>,
    acting_task: Option<veoveo_hub_sdk::OpaqueTaskId>,
    delegation_context: Option<veoveo_hub_sdk::DelegationContext>,
    delegation_expires_at: Option<chrono::DateTime<Utc>>,
}

impl std::fmt::Debug for HubAuthorizedHttpClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HubAuthorizedHttpClient")
            .field("profile", &self.profile)
            .field("server", &self.server)
            .field("actor", &self.actor.id)
            .field("authority", &self.authority)
            .finish_non_exhaustive()
    }
}

impl HubAuthorizedHttpClient {
    /// `permissions` are the caller's permissions in `server`'s namespace, as the
    /// Hub granted them; the server checks them again.
    pub(super) fn new(
        http: UpstreamTransport,
        issuer: HubInternalTokenIssuer,
        profile: HubProfileId,
        server: &crate::registration::RegisteredServer,
        subject: &crate::AuthenticatedSubject,
        permissions: BTreeSet<PermissionName>,
    ) -> Result<Self, McpError> {
        Ok(Self {
            http,
            issuer,
            profile,
            server: server.slug.clone(),
            deployment: server.deployment,
            audience: ResourceIndicator::from(&server.endpoint),
            client: subject.access_token.oauth_client_id.clone(),
            actor: subject.actor.clone(),
            authority: subject.authority.clone(),
            permissions,
            request_context: subject
                .request_context()
                .map_err(|error| crate::mcp_support::mcp_internal(error.to_string()))?,
            acting_server: subject
                .delegation
                .as_ref()
                .map(|delegation| delegation.acting_server.clone()),
            acting_task: subject.acting_task().cloned(),
            delegation_context: subject
                .delegation
                .as_ref()
                .and_then(|delegation| delegation.context.clone()),
            delegation_expires_at: subject
                .delegation
                .as_ref()
                .map(|delegation| delegation.expires_at),
        })
    }

    /// The same authorization over a new client for the server, after a
    /// connection-level failure showed its pooled connections are stale.
    pub(super) fn reconnected(&self) -> Result<Self, McpError> {
        self.http.discard();
        Ok(Self {
            http: self.http.current()?,
            ..self.clone()
        })
    }

    /// Map a transport error, discarding the server's pooled client when the failure
    /// lies in the connection: the request could not be sent or its response broke off.
    fn transport_error(
        &self,
        error: StreamableHttpError<reqwest::Error>,
    ) -> StreamableHttpError<HubAuthorizedHttpError> {
        if let StreamableHttpError::Client(error) = &error
            && (error.is_connect() || error.is_request() || error.is_body())
        {
            self.http.discard();
        }
        map_reqwest_transport_error(error)
    }

    fn issue_bearer_token(&self) -> Result<String, HubAuthorizedHttpError> {
        self.issue_bearer_token_until(
            Utc::now() + TimeDelta::seconds(INTERNAL_REQUEST_TOKEN_TTL_SECONDS),
        )
    }

    fn issue_message_bearer_token(
        &self,
        message: &ClientJsonRpcMessage,
    ) -> Result<String, HubAuthorizedHttpError> {
        let lifetime = match message {
            ClientJsonRpcMessage::Request(request)
                if matches!(
                    request.request,
                    ClientRequest::SubscriptionsListenRequest(_)
                ) =>
            {
                INTERNAL_SUBSCRIPTION_TOKEN_TTL_SECONDS
            }
            _ => INTERNAL_REQUEST_TOKEN_TTL_SECONDS,
        };
        self.issue_bearer_token_until(Utc::now() + TimeDelta::seconds(lifetime))
    }

    fn issue_bearer_token_until(
        &self,
        expires_at: chrono::DateTime<Utc>,
    ) -> Result<String, HubAuthorizedHttpError> {
        // The issuer also caps this deadline at the signed source token expiry.
        self.issuer
            .issue(ForwardedRequest {
                profile: self.profile.clone(),
                server: self.server.clone(),
                deployment: self.deployment,
                audience: self.audience.clone(),
                client: self.client.clone(),
                actor: self.actor.clone(),
                authority: self.authority.clone(),
                permissions: self.permissions.clone(),
                request_context: Some(self.request_context.clone()),
                acting_server: self.acting_server.clone(),
                acting_task: self.acting_task.clone(),
                delegation_context: self.delegation_context.clone(),
                expires_at: self
                    .delegation_expires_at
                    .map_or(expires_at, |deadline| deadline.min(expires_at)),
            })
            .map(|issued| issued.bearer_token)
            .map_err(HubAuthorizedHttpError::InternalToken)
    }
}

#[derive(Debug, Error)]
pub(super) enum HubAuthorizedHttpError {
    #[error("failed to issue hub internal request token: {0}")]
    InternalToken(InternalTokenError),
    #[error("upstream HTTP request failed: {0}")]
    Http(reqwest::Error),
}

impl StreamableHttpClient for HubAuthorizedHttpClient {
    type Error = HubAuthorizedHttpError;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        _static_auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let bearer_token = self
            .issue_message_bearer_token(&message)
            .map_err(StreamableHttpError::Client)?;
        <reqwest::Client as StreamableHttpClient>::post_message(
            self.http.client(),
            uri,
            message,
            session_id,
            Some(bearer_token),
            custom_headers,
        )
        .await
        .map_err(|error| self.transport_error(error))
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        _static_auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>> {
        let bearer_token = self
            .issue_bearer_token()
            .map_err(StreamableHttpError::Client)?;
        <reqwest::Client as StreamableHttpClient>::delete_session(
            self.http.client(),
            uri,
            session_id,
            Some(bearer_token),
            custom_headers,
        )
        .await
        .map_err(|error| self.transport_error(error))
    }

    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        _static_auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, StreamableHttpError<Self::Error>> {
        let bearer_token = self
            .issue_bearer_token()
            .map_err(StreamableHttpError::Client)?;
        <reqwest::Client as StreamableHttpClient>::get_stream(
            self.http.client(),
            uri,
            session_id,
            last_event_id,
            Some(bearer_token),
            custom_headers,
        )
        .await
        .map_err(|error| self.transport_error(error))
    }
}

#[allow(clippy::needless_pass_by_value)]
fn map_reqwest_transport_error(
    error: StreamableHttpError<reqwest::Error>,
) -> StreamableHttpError<HubAuthorizedHttpError> {
    match error {
        StreamableHttpError::Sse(error) => StreamableHttpError::Sse(error),
        StreamableHttpError::Io(error) => StreamableHttpError::Io(error),
        StreamableHttpError::Client(error) => {
            StreamableHttpError::Client(HubAuthorizedHttpError::Http(error))
        }
        StreamableHttpError::UnexpectedEndOfStream => StreamableHttpError::UnexpectedEndOfStream,
        StreamableHttpError::UnexpectedServerResponse(response) => {
            StreamableHttpError::UnexpectedServerResponse(response)
        }
        StreamableHttpError::HttpResponse { status, body } => {
            StreamableHttpError::HttpResponse { status, body }
        }
        StreamableHttpError::UnexpectedContentType(content_type) => {
            StreamableHttpError::UnexpectedContentType(content_type)
        }
        StreamableHttpError::ServerDoesNotSupportSse => {
            StreamableHttpError::ServerDoesNotSupportSse
        }
        StreamableHttpError::ServerDoesNotSupportDeleteSession => {
            StreamableHttpError::ServerDoesNotSupportDeleteSession
        }
        StreamableHttpError::TokioJoinError(error) => StreamableHttpError::TokioJoinError(error),
        StreamableHttpError::Deserialize(error) => StreamableHttpError::Deserialize(error),
        StreamableHttpError::TransportChannelClosed => StreamableHttpError::TransportChannelClosed,
        StreamableHttpError::MissingSessionIdInResponse => {
            StreamableHttpError::MissingSessionIdInResponse
        }
        StreamableHttpError::AuthRequired(AuthRequiredError {
            www_authenticate_header,
            ..
        }) => StreamableHttpError::AuthRequired(AuthRequiredError::new(www_authenticate_header)),
        StreamableHttpError::InsufficientScope(InsufficientScopeError {
            www_authenticate_header,
            required_scope,
            ..
        }) => StreamableHttpError::InsufficientScope(InsufficientScopeError::new(
            www_authenticate_header,
            required_scope,
        )),
        StreamableHttpError::ReservedHeaderConflict(header) => {
            StreamableHttpError::ReservedHeaderConflict(header)
        }
        StreamableHttpError::SessionExpired => StreamableHttpError::SessionExpired,
        other => StreamableHttpError::UnexpectedServerResponse(other.to_string().into()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use veoveo_hub_sdk::{HubInternalSigningKey, PrincipalKind, TokenIssuer, TokenSubject};
    use veoveo_types::{
        AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, ScopeName, TenantId,
        WorkContextId,
    };
    use veoveo_types::{WorkContextMembershipLevel, WorkContextOutputPolicy};

    use super::*;

    const TEST_SIGNING_KEY_DER_B64: &str =
        "MC4CAQAwBQYDK2VwBCIEII4AsVspz8h7mpqvOkgslJP07HfqpiWMZA+6Ii90lVBl";

    const TEST_ISSUER: &str = "https://hub.veoveo.test";
    const TEST_RESOURCE: &str = "https://upstream-b.veoveo.test/mcp";

    fn signing_key() -> HubInternalSigningKey {
        HubInternalSigningKey::new(
            base64::engine::general_purpose::STANDARD
                .decode(TEST_SIGNING_KEY_DER_B64)
                .unwrap(),
        )
        .unwrap()
    }

    fn registered_server() -> crate::registration::RegisteredServer {
        crate::registration::RegisteredServer {
            deployment: veoveo_hub_sdk::hosting::testing::deployment_id(),
            slug: ServerSlug::parse("upstream-b").unwrap(),
            uri_scheme: veoveo_types::ResourceScheme::parse("upstream-b").unwrap(),
            endpoint: veoveo_hub_sdk::registration::ServerEndpoint::new(
                url::Url::parse(TEST_RESOURCE).unwrap(),
            )
            .unwrap(),
            version: veoveo_hub_sdk::registration::ServerVersion::new("0.1.0".to_owned()).unwrap(),
            client_id: OAuthClientId::parse("upstream-b-0001").unwrap(),
            authorization: std::sync::Arc::new(
                veoveo_hub_sdk::registration::AuthorizationDeclaration {
                    permissions: Vec::new(),
                    requirements: Default::default(),
                    uses: Vec::new(),
                    calls_hub: false,
                },
            ),
            capabilities: Default::default(),
            branch: None,
        }
    }

    fn client() -> HubAuthorizedHttpClient {
        let issuer =
            HubInternalTokenIssuer::new(TokenIssuer::parse(TEST_ISSUER).unwrap(), signing_key())
                .unwrap();
        let actor_id = PrincipalId::parse("https://identity.example#operator").unwrap();
        let actor = Principal {
            id: actor_id.clone(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::parse("https://identity.example").unwrap(),
            subject: TokenSubject::parse("operator").unwrap(),
            tenant: Some(TenantId::parse("tenant").unwrap()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::from([ScopeName::parse("operator:use").unwrap()]),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: None,
        };
        let authority = InvocationAuthority {
            work_context: WorkContextId::parse("mission").unwrap(),
            tenant: TenantId::parse("tenant").unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::parse("r1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(actor_id.clone()),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Direct {
                initiator: actor_id,
            },
        };
        HubAuthorizedHttpClient::new(
            super::super::HubUpstreamHttpClientPool::new()
                .transport(
                    registered_server().deployment,
                    &registered_server().endpoint,
                    false,
                )
                .unwrap(),
            issuer,
            HubProfileId::parse("operator").unwrap(),
            &registered_server(),
            &crate::AuthenticatedSubject {
                execution: Some(veoveo_hub_sdk::hosting::testing::execution_context()),
                extensions: Default::default(),
                audit: veoveo_audit_contract::AuditRequest::background(),
                access_token: veoveo_hub_sdk::AccessTokenSubject {
                    issuer: actor.issuer.clone(),
                    subject: actor.subject.clone(),
                    oauth_client_id: veoveo_hub_sdk::OAuthClientId::parse("console").unwrap(),
                    session_family: Some(
                        veoveo_hub_sdk::HubRefreshFamilyId::parse(uuid::Uuid::now_v7().to_string())
                            .unwrap(),
                    ),
                    audience: veoveo_hub_sdk::ProtectedResourceId::parse("operator").unwrap(),
                    work_context: authority.work_context.clone(),
                    invocation_mode: veoveo_types::InvocationMode::Direct,
                    initiator: Some(actor.id.clone()),
                    delegation_id: None,
                    scopes: actor.scopes.clone(),
                    jwt_id: None,
                    issued_at: Utc::now(),
                    not_before: None,
                    expires_at: Utc::now() + TimeDelta::minutes(15),
                },
                principal: actor.clone(),
                actor,
                principal_display_name: None,
                authority,
                delegation: None,
            },
            BTreeSet::from([PermissionName::parse("upstream-b.items.read").unwrap()]),
        )
        .unwrap()
    }

    #[test]
    fn each_http_request_receives_a_fresh_short_lived_assertion() {
        let client = client();
        let first = client.issue_bearer_token().unwrap();
        let second = client.issue_bearer_token().unwrap();
        assert_ne!(first, second);

        for token in [first, second] {
            let payload = token.split('.').nth(1).unwrap();
            let claims: serde_json::Value =
                serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap();
            assert_eq!(
                claims["exp"].as_i64().unwrap() - claims["iat"].as_i64().unwrap(),
                INTERNAL_REQUEST_TOKEN_TTL_SECONDS
            );
            assert_eq!(claims["aud"], TEST_RESOURCE);
            assert_eq!(claims["client_id"], "console");
            assert_eq!(
                claims["authorization_details"][0]["actions"],
                serde_json::json!(["upstream-b.items.read"])
            );
        }
    }

    #[test]
    fn delegated_forwarding_preserves_signed_origin_and_cannot_renew_deadline() {
        use veoveo_hub_sdk::{
            ActingServer, DelegationContext, HubInternalTokenVerifier, HubInternalTrustBundle,
            TokenAudience,
        };
        let baseline = client();
        let original = baseline.request_context.clone();
        let deadline = Utc::now() + TimeDelta::seconds(12);
        let acting = ActingServer {
            deployment: veoveo_hub_sdk::DeploymentId::new(),
            server: ServerSlug::parse("middle").unwrap(),
            client: OAuthClientId::parse("middle-1").unwrap(),
        };
        let context = DelegationContext {
            actors: vec![ActingServer {
                deployment: veoveo_hub_sdk::DeploymentId::new(),
                server: ServerSlug::parse("first").unwrap(),
                client: OAuthClientId::parse("first-1").unwrap(),
            }],
            task: None,
            expires_at: deadline,
        };
        let subject = crate::AuthenticatedSubject {
            execution: Some(veoveo_hub_sdk::hosting::testing::execution_context()),
            extensions: Default::default(),
            audit: veoveo_audit_contract::AuditRequest::background(),
            access_token: original.access_token.clone(),
            principal: original.principal.clone(),
            actor: baseline.actor.clone(),
            principal_display_name: None,
            authority: baseline.authority.clone(),
            delegation: Some(crate::delegation::SubjectDelegation {
                acting_server: acting.clone(),
                permissions: baseline.permissions.clone(),
                task: None,
                request_context: original.clone(),
                context: Some(context.clone()),
                expires_at: deadline,
            }),
        };
        assert_ne!(subject.audit, original.audit);
        assert_eq!(subject.request_context().unwrap(), original);
        let mut delegated = HubAuthorizedHttpClient::new(
            baseline.http,
            baseline.issuer,
            baseline.profile,
            &registered_server(),
            &subject,
            baseline.permissions,
        )
        .unwrap();
        let verifier = HubInternalTokenVerifier::new(
            TokenIssuer::parse(TEST_ISSUER).unwrap(),
            TokenAudience {
                deployment: delegated.deployment,
                server: delegated.server.clone(),
                resource: delegated.audience.clone(),
            },
            HubInternalTrustBundle::from_key_set(
                &veoveo_hub_sdk::registration::Ed25519JwkSet::new(vec![
                    signing_key().verification_key(),
                ])
                .unwrap(),
            )
            .unwrap(),
        );
        for lifetime in [60, 900] {
            let token = delegated
                .issue_bearer_token_until(Utc::now() + TimeDelta::seconds(lifetime))
                .unwrap();
            let identity = verifier.verify(&token).unwrap();
            assert_eq!(identity.request_context, Some(original.clone()));
            assert_eq!(identity.actor, subject.actor);
            assert_eq!(identity.authority, subject.authority);
            assert_eq!(identity.acting_server, Some(acting.clone()));
            assert_eq!(identity.delegation_context, Some(context.clone()));
            assert!(identity.expires_at <= deadline);
        }
        delegated.delegation_expires_at = Some(Utc::now() - TimeDelta::seconds(1));
        assert!(delegated.issue_bearer_token().is_err());
    }

    #[tokio::test]
    async fn subscription_post_uses_caller_bounded_lifetime_without_extending_other_requests() {
        use axum::{Json, Router, extract::State, http::HeaderMap, routing::post};
        use rmcp::model::{
            JsonRpcRequest, PingRequest, SubscriptionFilter, SubscriptionsListenRequest,
            SubscriptionsListenRequestParams,
        };
        use tokio::sync::mpsc;
        use veoveo_hub_sdk::{
            HubInternalIdentity, HubInternalTokenVerifier, HubInternalTrustBundle,
        };

        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let verifier = HubInternalTokenVerifier::new(
                TokenIssuer::parse(TEST_ISSUER).unwrap(),
                veoveo_hub_sdk::TokenAudience {
                    deployment: registered_server().deployment,
                    server: ServerSlug::parse("upstream-b").unwrap(),
                    resource: ResourceIndicator::parse(TEST_RESOURCE).unwrap(),
                },
                HubInternalTrustBundle::from_key_set(
                    &veoveo_hub_sdk::registration::Ed25519JwkSet::new(vec![
                        signing_key().verification_key(),
                    ])
                    .unwrap(),
                )
                .unwrap(),
            );
            let (send, mut received) = mpsc::channel::<HubInternalIdentity>(4);
            let router = Router::new()
                .route(
                    "/",
                    post(
                        |State((verifier, send)): State<(
                            HubInternalTokenVerifier,
                            mpsc::Sender<HubInternalIdentity>,
                        )>,
                         headers: HeaderMap,
                         Json(message): Json<ClientJsonRpcMessage>| async move {
                            let bearer = headers["authorization"]
                                .to_str()
                                .unwrap()
                                .strip_prefix("Bearer ")
                                .unwrap();
                            send.send(verifier.verify(bearer).unwrap()).await.unwrap();
                            let ClientJsonRpcMessage::Request(request) = message else {
                                panic!("expected a request")
                            };
                            Json(serde_json::json!({"jsonrpc":"2.0", "id":request.id, "result":{}}))
                        },
                    ),
                )
                .with_state((verifier, send));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint: Arc<str> = format!("http://{}/", listener.local_addr().unwrap()).into();
            let stop = tokio_util::sync::CancellationToken::new();
            let _stop_on_drop = stop.clone().drop_guard();
            let cancellation = stop.clone();
            let server = tokio::spawn(async move {
                axum::serve(listener, router)
                    .with_graceful_shutdown(cancellation.cancelled_owned())
                    .await
                    .unwrap();
            });
            let mut client = client();
            for (subscription, caller_seconds, expected_seconds) in [
                (false, 3600, 60),
                (true, 3600, 900),
                (true, 120, 120),
                (false, 20, 20),
            ] {
                client.request_context.access_token.expires_at =
                    Utc::now() + TimeDelta::seconds(caller_seconds);
                let request = if subscription {
                    ClientRequest::SubscriptionsListenRequest(SubscriptionsListenRequest::new(
                        SubscriptionsListenRequestParams::new(
                            SubscriptionFilter::builder()
                                .resources_list_changed()
                                .build(),
                        ),
                    ))
                } else {
                    ClientRequest::PingRequest(PingRequest::default())
                };
                let message = ClientJsonRpcMessage::Request(JsonRpcRequest::new(
                    rmcp::model::RequestId::Number(1),
                    request,
                ));
                client
                    .post_message(endpoint.clone(), message, None, None, HashMap::new())
                    .await
                    .unwrap();
                let identity = received.recv().await.unwrap();
                assert!(identity.expires_at <= client.request_context.access_token.expires_at);
                assert_eq!(
                    identity.expires_at.timestamp() - identity.issued_at.timestamp(),
                    expected_seconds
                );
                assert_eq!(identity.actor, client.actor);
                assert_eq!(identity.authority, client.authority);
            }
            client.request_context.access_token.expires_at = Utc::now() - TimeDelta::seconds(1);
            assert!(client.issue_bearer_token().is_err());
            stop.cancel();
            server.await.unwrap();
        })
        .await
        .expect("HTTP assertion lifetime check exceeded ten seconds");
    }
}
