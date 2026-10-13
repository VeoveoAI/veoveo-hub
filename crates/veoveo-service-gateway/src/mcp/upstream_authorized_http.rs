use std::{collections::HashMap, sync::Arc};

use axum::http::{HeaderName, HeaderValue};
use chrono::{TimeDelta, Utc};
use futures::stream::BoxStream;
use rmcp::{
    model::{ClientJsonRpcMessage, ClientRequest},
    transport::streamable_http_client::{
        AuthRequiredError, InsufficientScopeError, SseError, StreamableHttpClient,
        StreamableHttpError, StreamableHttpPostResponse,
    },
};
use sse_stream::Sse;
use std::collections::BTreeSet;
use thiserror::Error;
use veoveo_sdk_rust::registration::ResourceIndicator;
use veoveo_sdk_rust::{
    ForwardedRequest, GatewayInternalTokenIssuer, GatewayProfileId, GatewayRequestContext,
    InternalTokenError, Principal, ServerSlug,
};
use veoveo_types::{InvocationAuthority, OAuthClientId, PermissionName};

const INTERNAL_REQUEST_TOKEN_TTL_SECONDS: i64 = 60;
const INTERNAL_SUBSCRIPTION_TOKEN_TTL_SECONDS: i64 = 15 * 60;

/// Per-request HTTP authorization for one auth-scoped gateway-to-server client.
///
/// This client signs a lifetime-bounded assertion for every request while retaining
/// one immutable invocation authority. Final-profile POSTs and request-scoped
/// listener streams therefore never derive authority from connection locality.
#[derive(Clone)]
pub(super) struct GatewayAuthorizedHttpClient {
    http: reqwest::Client,
    issuer: GatewayInternalTokenIssuer,
    profile: GatewayProfileId,
    server: ServerSlug,
    audience: ResourceIndicator,
    client: OAuthClientId,
    actor: Principal,
    authority: InvocationAuthority,
    permissions: BTreeSet<PermissionName>,
    request_context: GatewayRequestContext,
    acting_server: Option<veoveo_sdk_rust::ActingServer>,
    acting_task: Option<veoveo_sdk_rust::OpaqueTaskId>,
}

impl std::fmt::Debug for GatewayAuthorizedHttpClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GatewayAuthorizedHttpClient")
            .field("profile", &self.profile)
            .field("server", &self.server)
            .field("actor", &self.actor.id)
            .field("authority", &self.authority)
            .finish_non_exhaustive()
    }
}

impl GatewayAuthorizedHttpClient {
    /// `permissions` are the caller's permissions in `server`'s namespace, as the
    /// Gateway granted them; the server checks them again.
    pub(super) fn new(
        http: reqwest::Client,
        issuer: GatewayInternalTokenIssuer,
        profile: GatewayProfileId,
        server: &crate::registration::RegisteredServer,
        subject: &crate::AuthenticatedSubject,
        permissions: BTreeSet<PermissionName>,
    ) -> Self {
        Self {
            http,
            issuer,
            profile,
            server: server.slug.clone(),
            audience: ResourceIndicator::from(&server.endpoint),
            client: subject.access_token.oauth_client_id.clone(),
            actor: subject.actor.clone(),
            authority: subject.authority.clone(),
            permissions,
            request_context: subject.request_context(),
            acting_server: subject
                .delegation
                .as_ref()
                .map(|delegation| delegation.acting_server.clone()),
            acting_task: subject.acting_task().cloned(),
        }
    }

    fn issue_bearer_token(&self) -> Result<String, GatewayAuthorizedHttpError> {
        self.issue_bearer_token_until(
            Utc::now() + TimeDelta::seconds(INTERNAL_REQUEST_TOKEN_TTL_SECONDS),
        )
    }

    fn issue_message_bearer_token(
        &self,
        message: &ClientJsonRpcMessage,
    ) -> Result<String, GatewayAuthorizedHttpError> {
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
    ) -> Result<String, GatewayAuthorizedHttpError> {
        // The issuer also caps this deadline at the signed source token expiry.
        self.issuer
            .issue(ForwardedRequest {
                profile: self.profile.clone(),
                server: self.server.clone(),
                audience: self.audience.clone(),
                client: self.client.clone(),
                actor: self.actor.clone(),
                authority: self.authority.clone(),
                permissions: self.permissions.clone(),
                request_context: Some(self.request_context.clone()),
                acting_server: self.acting_server.clone(),
                acting_task: self.acting_task.clone(),
                expires_at,
            })
            .map(|issued| issued.bearer_token)
            .map_err(GatewayAuthorizedHttpError::InternalToken)
    }
}

#[derive(Debug, Error)]
pub(super) enum GatewayAuthorizedHttpError {
    #[error("failed to issue gateway internal request token: {0}")]
    InternalToken(InternalTokenError),
    #[error("upstream HTTP request failed: {0}")]
    Http(reqwest::Error),
}

impl StreamableHttpClient for GatewayAuthorizedHttpClient {
    type Error = GatewayAuthorizedHttpError;

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
            &self.http,
            uri,
            message,
            session_id,
            Some(bearer_token),
            custom_headers,
        )
        .await
        .map_err(map_reqwest_transport_error)
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
            &self.http,
            uri,
            session_id,
            Some(bearer_token),
            custom_headers,
        )
        .await
        .map_err(map_reqwest_transport_error)
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
            &self.http,
            uri,
            session_id,
            last_event_id,
            Some(bearer_token),
            custom_headers,
        )
        .await
        .map_err(map_reqwest_transport_error)
    }
}

#[allow(clippy::needless_pass_by_value)]
fn map_reqwest_transport_error(
    error: StreamableHttpError<reqwest::Error>,
) -> StreamableHttpError<GatewayAuthorizedHttpError> {
    match error {
        StreamableHttpError::Sse(error) => StreamableHttpError::Sse(error),
        StreamableHttpError::Io(error) => StreamableHttpError::Io(error),
        StreamableHttpError::Client(error) => {
            StreamableHttpError::Client(GatewayAuthorizedHttpError::Http(error))
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
    use veoveo_sdk_rust::{GatewayInternalSigningKey, PrincipalKind, TokenIssuer, TokenSubject};
    use veoveo_types::{
        AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, ScopeName, TenantId,
        WorkContextId,
    };
    use veoveo_types::{WorkContextMembershipLevel, WorkContextOutputPolicy};

    use super::*;

    const TEST_SIGNING_KEY_DER_B64: &str =
        "MC4CAQAwBQYDK2VwBCIEII4AsVspz8h7mpqvOkgslJP07HfqpiWMZA+6Ii90lVBl";

    const TEST_ISSUER: &str = "https://gateway.veoveo.test";
    const TEST_RESOURCE: &str = "https://upstream-b.veoveo.test/mcp";

    fn signing_key() -> GatewayInternalSigningKey {
        GatewayInternalSigningKey::new(
            base64::engine::general_purpose::STANDARD
                .decode(TEST_SIGNING_KEY_DER_B64)
                .unwrap(),
        )
        .unwrap()
    }

    fn registered_server() -> crate::registration::RegisteredServer {
        crate::registration::RegisteredServer {
            slug: ServerSlug::parse("upstream-b").unwrap(),
            uri_scheme: veoveo_types::ResourceScheme::parse("upstream-b").unwrap(),
            endpoint: veoveo_sdk_rust::registration::ServerEndpoint::new(
                url::Url::parse(TEST_RESOURCE).unwrap(),
            )
            .unwrap(),
            version: veoveo_sdk_rust::registration::ServerVersion::new("0.1.0".to_owned()).unwrap(),
            client_id: OAuthClientId::parse("upstream-b-0001").unwrap(),
            authorization: std::sync::Arc::new(
                veoveo_sdk_rust::registration::AuthorizationDeclaration {
                    permissions: Vec::new(),
                    requirements: Default::default(),
                    uses: Vec::new(),
                },
            ),
            capabilities: Default::default(),
        }
    }

    fn client() -> GatewayAuthorizedHttpClient {
        let issuer = GatewayInternalTokenIssuer::new(
            TokenIssuer::parse(TEST_ISSUER).unwrap(),
            signing_key(),
        )
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
        GatewayAuthorizedHttpClient::new(
            reqwest::Client::new(),
            issuer,
            GatewayProfileId::parse("operator").unwrap(),
            &registered_server(),
            &crate::AuthenticatedSubject {
                extensions: Default::default(),
                audit: veoveo_audit_contract::AuditRequest::background(),
                access_token: veoveo_sdk_rust::AccessTokenSubject {
                    issuer: actor.issuer.clone(),
                    subject: actor.subject.clone(),
                    oauth_client_id: veoveo_sdk_rust::OAuthClientId::parse("console").unwrap(),
                    session_family: Some(
                        veoveo_sdk_rust::GatewayRefreshFamilyId::parse(
                            uuid::Uuid::now_v7().to_string(),
                        )
                        .unwrap(),
                    ),
                    audience: veoveo_sdk_rust::ProtectedResourceId::parse("operator").unwrap(),
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

    #[tokio::test]
    async fn subscription_post_uses_caller_bounded_lifetime_without_extending_other_requests() {
        use axum::{Json, Router, extract::State, http::HeaderMap, routing::post};
        use rmcp::model::{
            JsonRpcRequest, PingRequest, SubscriptionFilter, SubscriptionsListenRequest,
            SubscriptionsListenRequestParams,
        };
        use tokio::sync::mpsc;
        use veoveo_sdk_rust::{
            GatewayInternalIdentity, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
        };

        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let verifier = GatewayInternalTokenVerifier::new(
                TokenIssuer::parse(TEST_ISSUER).unwrap(),
                veoveo_sdk_rust::TokenAudience {
                    server: ServerSlug::parse("upstream-b").unwrap(),
                    resource: ResourceIndicator::parse(TEST_RESOURCE).unwrap(),
                },
                GatewayInternalTrustBundle::from_key_set(
                    &veoveo_sdk_rust::registration::Ed25519JwkSet::new(vec![
                        signing_key().verification_key(),
                    ])
                    .unwrap(),
                )
                .unwrap(),
            );
            let (send, mut received) = mpsc::channel::<GatewayInternalIdentity>(4);
            let router = Router::new()
                .route(
                    "/",
                    post(
                        |State((verifier, send)): State<(
                            GatewayInternalTokenVerifier,
                            mpsc::Sender<GatewayInternalIdentity>,
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
