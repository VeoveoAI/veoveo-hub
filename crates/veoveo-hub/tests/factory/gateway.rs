//! A fake OpenShell gateway: an in-process gRPC server that answers the requests the
//! factory sends from sandboxes and providers it keeps in memory, and records every
//! request that changes them. Like the real gateway, it records a created sandbox's
//! policy in its canonical form and refuses `UpdateConfig`, which needs the
//! `config:write` scope the Hub's identity does not hold. It stops a sandbox that is
//! ready, stopping, failed or in an automatic restart, and resets its restart count,
//! answers a stop of a stopped or completed sandbox without change and refuses one still
//! provisioning or starting for the first time; it starts a stopped, completed, failed
//! or starting sandbox at once and answers a start of a ready one without change. Any
//! other method is unimplemented. Once given the token endpoint of a profile's
//! `oauth2_client_credentials` refresh, a rotation mints the credential there with the
//! provider's refresh material. It serves plain gRPC to the test and TLS to the Hub
//! binary.

use std::{
    collections::{BTreeMap, HashMap},
    convert::Infallible,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use openshell_sdk::{EdgeAuthInterceptor, OpenShellClient, raw::proto as p};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{
    Status,
    body::Body,
    codegen::http,
    server::{Grpc, NamedService, UnaryService},
};
use tonic_prost::ProstCodec;

/// A request that changes the gateway's state.
#[derive(Debug, Clone, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "a test records a handful of requests"
)]
pub enum Call {
    CreateProvider(p::CreateProviderRequest),
    DeleteProvider(p::DeleteProviderRequest),
    CreateSandbox(p::CreateSandboxRequest),
    StopSandbox(p::StopSandboxRequest),
    StartSandbox(p::StartSandboxRequest),
    DeleteSandbox(p::DeleteSandboxRequest),
    ConfigureProviderRefresh(p::ConfigureProviderRefreshRequest),
    RotateProviderCredential(p::RotateProviderCredentialRequest),
}

#[derive(Default)]
struct State {
    calls: Vec<Call>,
    /// By workspace and name.
    sandboxes: BTreeMap<(String, String), p::Sandbox>,
    providers: BTreeMap<(String, String), p::Provider>,
    /// The refusal the next request of a method answers instead, by method.
    refusals: BTreeMap<String, Status>,
    /// Refresh material by workspace, provider and credential key.
    refresh: BTreeMap<(String, String, String), std::collections::HashMap<String, String>>,
    /// Where a rotation mints, when set.
    token_url: Option<String>,
}

#[derive(Clone, Default)]
pub struct Gateway {
    state: Arc<Mutex<State>>,
}

impl Gateway {
    /// Serve the fake on a loopback port, and a client of it.
    pub async fn start() -> (Self, OpenShellClient) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let gateway = Self::default();
        tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(gateway.clone())
                .serve_with_incoming(TcpListenerStream::new(listener)),
        );
        let channel = tonic::transport::Endpoint::from_shared(format!("http://{address}"))
            .unwrap()
            .connect_lazy();
        (
            gateway,
            OpenShellClient::from_parts(channel, EdgeAuthInterceptor::noop()),
        )
    }

    /// Serve the same gateway over TLS on another loopback port, with a certificate for
    /// 127.0.0.1 that a new authority signs. Returns its URL and the authority's PEM.
    pub async fn serve_tls(&self) -> (url::Url, String) {
        let authority_key = rcgen::KeyPair::generate().unwrap();
        let mut authority = rcgen::CertificateParams::new(Vec::new()).unwrap();
        authority.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        authority
            .distinguished_name
            .push(rcgen::DnType::CommonName, "Fake gateway authority");
        let authority_certificate = authority.self_signed(&authority_key).unwrap();
        let issuer = rcgen::Issuer::new(authority, authority_key);
        let key = rcgen::KeyPair::generate().unwrap();
        let certificate = rcgen::CertificateParams::new(vec!["127.0.0.1".to_owned()])
            .unwrap()
            .signed_by(&key, &issuer)
            .unwrap();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let tls = tonic::transport::ServerTlsConfig::new().identity(
            tonic::transport::Identity::from_pem(certificate.pem(), key.serialize_pem()),
        );
        tokio::spawn(
            tonic::transport::Server::builder()
                .tls_config(tls)
                .unwrap()
                .add_service(self.clone())
                .serve_with_incoming(TcpListenerStream::new(listener)),
        );
        (
            url::Url::parse(&format!("https://{address}/")).unwrap(),
            authority_certificate.pem(),
        )
    }

    /// Answer the next request of `method` with `refusal`.
    pub fn refuse_next(&self, method: &str, refusal: Status) {
        self.state
            .lock()
            .unwrap()
            .refusals
            .insert(method.into(), refusal);
    }

    /// The credentials of a provider.
    pub fn credentials(&self, workspace: &str, name: &str) -> Option<HashMap<String, String>> {
        self.state
            .lock()
            .unwrap()
            .providers
            .get(&(workspace.into(), name.into()))
            .map(|provider| provider.credentials.clone())
    }

    /// The recorded requests, which the next call no longer returns.
    pub fn take_calls(&self) -> Vec<Call> {
        std::mem::take(&mut self.state.lock().unwrap().calls)
    }

    pub fn sandbox(&self, workspace: &str, name: &str) -> Option<p::Sandbox> {
        self.state
            .lock()
            .unwrap()
            .sandboxes
            .get(&(workspace.into(), name.into()))
            .cloned()
    }

    pub fn providers(&self, workspace: &str) -> Vec<String> {
        self.state
            .lock()
            .unwrap()
            .providers
            .keys()
            .filter(|(owner, _)| owner == workspace)
            .map(|(_, name)| name.clone())
            .collect()
    }

    /// Mint rotated credentials at `url` with OAuth 2.0 client credentials, as the
    /// gateway does for a profile whose refresh names that token endpoint.
    pub fn set_token_url(&self, url: String) {
        self.state.lock().unwrap().token_url = Some(url);
    }

    /// The refresh material configured for a provider's credential.
    pub fn refresh_material(
        &self,
        workspace: &str,
        provider: &str,
        key: &str,
    ) -> Option<std::collections::HashMap<String, String>> {
        self.state
            .lock()
            .unwrap()
            .refresh
            .get(&(workspace.into(), provider.into(), key.into()))
            .cloned()
    }

    /// Replace a sandbox's status, as the compute platform's observations would.
    pub fn set_status(&self, workspace: &str, name: &str, status: p::SandboxStatus) {
        self.state
            .lock()
            .unwrap()
            .sandboxes
            .get_mut(&(workspace.into(), name.into()))
            .expect("the sandbox exists")
            .status = Some(status);
    }

    pub fn set_phase(&self, workspace: &str, name: &str, phase: p::SandboxPhase) {
        self.set_status(
            workspace,
            name,
            p::SandboxStatus {
                phase: phase as i32,
                ..Default::default()
            },
        );
    }

    fn handle(&self, method: &str, request: http::Request<Body>) -> BoxFuture {
        let state = self.state.clone();
        if let Some(refusal) = state.lock().unwrap().refusals.remove(method) {
            return Box::pin(async move { refusal.into_http() });
        }
        match method {
            "CreateProvider" => unary(request, move |request: p::CreateProviderRequest| {
                let mut state = state.lock().unwrap();
                let provider = request.provider.clone().unwrap_or_default();
                let name = provider
                    .metadata
                    .as_ref()
                    .map(|m| m.name.clone())
                    .unwrap_or_default();
                let key = (workspace(request.workspace_scope.as_ref())?, name);
                if state.providers.contains_key(&key) {
                    return Err(Status::already_exists("provider exists"));
                }
                state.providers.insert(key, provider);
                state.calls.push(Call::CreateProvider(request));
                Ok(p::ProviderResponse::default())
            }),
            "DeleteProvider" => unary(request, move |request: p::DeleteProviderRequest| {
                let mut state = state.lock().unwrap();
                let key = (
                    workspace(request.workspace_scope.as_ref())?,
                    request.name.clone(),
                );
                if state.providers.remove(&key).is_none() && !request.allow_missing {
                    return Err(Status::not_found("provider not found"));
                }
                state.calls.push(Call::DeleteProvider(request));
                Ok(p::DeleteProviderResponse::default())
            }),
            "CreateSandbox" => unary(request, move |request: p::CreateSandboxRequest| {
                let mut state = state.lock().unwrap();
                let key = (
                    workspace(request.workspace_scope.as_ref())?,
                    request.name.clone(),
                );
                if state.sandboxes.contains_key(&key) {
                    return Err(Status::already_exists("sandbox exists"));
                }
                for provider in request.spec.iter().flat_map(|spec| &spec.providers) {
                    if !state
                        .providers
                        .contains_key(&(key.0.clone(), provider.clone()))
                    {
                        return Err(Status::failed_precondition("provider not found"));
                    }
                }
                let mut spec = request.spec.clone();
                if let Some(policy) = spec.as_mut().and_then(|spec| spec.policy.as_mut()) {
                    *policy =
                        openshell_policy::validate_and_canonicalize_sandbox_policy(policy.clone())
                            .map_err(|error| Status::invalid_argument(error.to_string()))?;
                }
                let sandbox = p::Sandbox {
                    metadata: Some(p::ObjectMeta {
                        // The real gateway assigns every sandbox an ID.
                        id: format!("{}-id", key.1),
                        name: key.1.clone(),
                        workspace: key.0.clone(),
                        ..Default::default()
                    }),
                    spec,
                    status: Some(p::SandboxStatus {
                        phase: p::SandboxPhase::Provisioning as i32,
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                state.sandboxes.insert(key, sandbox.clone());
                state.calls.push(Call::CreateSandbox(request));
                Ok(p::SandboxResponse {
                    sandbox: Some(sandbox),
                    ..Default::default()
                })
            }),
            "GetSandbox" => unary(request, move |request: p::GetSandboxRequest| {
                let state = state.lock().unwrap();
                let key = (workspace(request.workspace_scope.as_ref())?, request.name);
                let sandbox = state
                    .sandboxes
                    .get(&key)
                    .cloned()
                    .ok_or_else(|| Status::not_found("sandbox not found"))?;
                Ok(p::SandboxResponse {
                    sandbox: Some(sandbox),
                    ..Default::default()
                })
            }),
            "UpdateConfig" => unary(request, |_: p::UpdateConfigRequest| {
                Err::<p::UpdateConfigResponse, _>(Status::permission_denied(
                    "scope 'config:write' required",
                ))
            }),
            "StopSandbox" => unary(request, move |request: p::StopSandboxRequest| {
                let mut state = state.lock().unwrap();
                let key = (
                    workspace(request.workspace_scope.as_ref())?,
                    request.name.clone(),
                );
                let sandbox = state
                    .sandboxes
                    .get_mut(&key)
                    .ok_or_else(|| Status::not_found("sandbox not found"))?;
                let status = sandbox.status.get_or_insert_default();
                let phase = p::SandboxPhase::try_from(status.phase).unwrap_or_default();
                let automatic_restart =
                    phase == p::SandboxPhase::Starting && status.restart_count > 0;
                match phase {
                    p::SandboxPhase::Stopped | p::SandboxPhase::Completed => {}
                    p::SandboxPhase::Ready | p::SandboxPhase::Stopping | p::SandboxPhase::Error => {
                        status.phase = p::SandboxPhase::Stopped as i32;
                        status.restart_count = 0;
                    }
                    _ if automatic_restart => {
                        status.phase = p::SandboxPhase::Stopped as i32;
                        status.restart_count = 0;
                    }
                    _ => {
                        return Err(Status::failed_precondition(format!(
                            "sandbox must be Ready, Error, or in an automatic restart to stop (current phase: {phase:?})"
                        )));
                    }
                }
                let sandbox = sandbox.clone();
                state.calls.push(Call::StopSandbox(request));
                Ok(p::SandboxResponse {
                    sandbox: Some(sandbox),
                    ..Default::default()
                })
            }),
            "StartSandbox" => unary(request, move |request: p::StartSandboxRequest| {
                let mut state = state.lock().unwrap();
                let key = (
                    workspace(request.workspace_scope.as_ref())?,
                    request.name.clone(),
                );
                let sandbox = state
                    .sandboxes
                    .get_mut(&key)
                    .ok_or_else(|| Status::not_found("sandbox not found"))?;
                let status = sandbox.status.get_or_insert_default();
                match p::SandboxPhase::try_from(status.phase).unwrap_or_default() {
                    p::SandboxPhase::Ready => {}
                    p::SandboxPhase::Stopped
                    | p::SandboxPhase::Completed
                    | p::SandboxPhase::Starting
                    | p::SandboxPhase::Error => status.phase = p::SandboxPhase::Starting as i32,
                    phase => {
                        return Err(Status::failed_precondition(format!(
                            "sandbox must be Stopped, Completed or a failed main-process Error to start (current phase: {phase:?})"
                        )));
                    }
                }
                let sandbox = sandbox.clone();
                state.calls.push(Call::StartSandbox(request));
                Ok(p::SandboxResponse {
                    sandbox: Some(sandbox),
                    ..Default::default()
                })
            }),
            "DeleteSandbox" => unary(request, move |request: p::DeleteSandboxRequest| {
                let mut state = state.lock().unwrap();
                let key = (
                    workspace(request.workspace_scope.as_ref())?,
                    request.name.clone(),
                );
                if state.sandboxes.remove(&key).is_none() && !request.allow_missing {
                    return Err(Status::not_found("sandbox not found"));
                }
                state.calls.push(Call::DeleteSandbox(request));
                Ok(p::DeleteSandboxResponse::default())
            }),
            "ConfigureProviderRefresh" => unary(
                request,
                move |request: p::ConfigureProviderRefreshRequest| {
                    let mut state = state.lock().unwrap();
                    let workspace = workspace(request.workspace_scope.as_ref())?;
                    if !state
                        .providers
                        .contains_key(&(workspace.clone(), request.provider.clone()))
                    {
                        return Err(Status::not_found("provider not found"));
                    }
                    if request.strategy
                        != p::ProviderCredentialRefreshStrategy::Oauth2ClientCredentials as i32
                    {
                        return Err(Status::unimplemented(
                            "the fake gateway refreshes only with client credentials",
                        ));
                    }
                    state.refresh.insert(
                        (
                            workspace,
                            request.provider.clone(),
                            request.credential_key.clone(),
                        ),
                        request.material.clone(),
                    );
                    state.calls.push(Call::ConfigureProviderRefresh(request));
                    Ok(p::ConfigureProviderRefreshResponse::default())
                },
            ),
            "RotateProviderCredential" => unary_async(
                request,
                move |request: p::RotateProviderCredentialRequest| async move {
                    let key = (
                        workspace(request.workspace_scope.as_ref())?,
                        request.provider.clone(),
                        request.credential_key.clone(),
                    );
                    let (material, token_url) = {
                        let mut state = state.lock().unwrap();
                        state.calls.push(Call::RotateProviderCredential(request));
                        let material = state.refresh.get(&key).cloned().ok_or_else(|| {
                            Status::failed_precondition("the credential has no refresh")
                        })?;
                        (material, state.token_url.clone())
                    };
                    let Some(token_url) = token_url else {
                        return Ok(p::RotateProviderCredentialResponse::default());
                    };
                    let form = url::form_urlencoded::Serializer::new(String::new())
                        .append_pair("grant_type", "client_credentials")
                        .append_pair("client_id", &material["client_id"])
                        .append_pair("client_secret", &material["client_secret"])
                        .finish();
                    let response = reqwest::Client::new()
                        .post(token_url)
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(form)
                        .send()
                        .await
                        .map_err(|error| Status::unavailable(error.to_string()))?;
                    let status = response.status();
                    let body: serde_json::Value = response
                        .json()
                        .await
                        .map_err(|error| Status::unavailable(error.to_string()))?;
                    if !status.is_success() {
                        return Err(Status::failed_precondition(format!(
                            "the token endpoint answered {status}: {body}"
                        )));
                    }
                    let token = body["access_token"]
                        .as_str()
                        .ok_or_else(|| Status::internal("no access token"))?
                        .to_owned();
                    let mut state = state.lock().unwrap();
                    let provider = state
                        .providers
                        .get_mut(&(key.0, key.1))
                        .ok_or_else(|| Status::not_found("provider not found"))?;
                    provider.credentials.insert(key.2, token);
                    Ok(p::RotateProviderCredentialResponse::default())
                },
            ),
            other => {
                let status = Status::unimplemented(format!("the fake gateway has no {other}"));
                Box::pin(async move { status.into_http() })
            }
        }
    }
}

/// The workspace a request selects; the factory always names one.
fn workspace(scope: Option<&p::WorkspaceSelector>) -> Result<String, Status> {
    match scope.and_then(|scope| scope.selection.as_ref()) {
        Some(p::workspace_selector::Selection::Workspace(name)) => Ok(name.clone()),
        _ => Err(Status::invalid_argument("a named workspace is required")),
    }
}

type BoxFuture = Pin<Box<dyn Future<Output = http::Response<Body>> + Send>>;

/// Answer one unary request with `handle`.
fn unary<Request, Response, Handle>(request: http::Request<Body>, handle: Handle) -> BoxFuture
where
    Request: prost::Message + Default + Send + 'static,
    Response: prost::Message + Send + 'static,
    Handle: FnOnce(Request) -> Result<Response, Status> + Send + 'static,
{
    struct Once<Handle>(Option<Handle>);
    impl<Request, Response, Handle> UnaryService<Request> for Once<Handle>
    where
        Handle: FnOnce(Request) -> Result<Response, Status>,
    {
        type Response = Response;
        type Future = std::future::Ready<Result<tonic::Response<Response>, Status>>;
        fn call(&mut self, request: tonic::Request<Request>) -> Self::Future {
            let handle = self.0.take().expect("a unary service answers once");
            std::future::ready(handle(request.into_inner()).map(tonic::Response::new))
        }
    }
    Box::pin(async move {
        Grpc::new(ProstCodec::<Response, Request>::default())
            .unary(Once(Some(handle)), request)
            .await
    })
}

/// Answer one unary request with `handle`, which may wait.
fn unary_async<Request, Response, Handle, Answer>(
    request: http::Request<Body>,
    handle: Handle,
) -> BoxFuture
where
    Request: prost::Message + Default + Send + 'static,
    Response: prost::Message + Send + 'static,
    Handle: FnOnce(Request) -> Answer + Send + 'static,
    Answer: Future<Output = Result<Response, Status>> + Send + 'static,
{
    struct Once<Handle>(Option<Handle>);
    impl<Request, Response, Handle, Answer> UnaryService<Request> for Once<Handle>
    where
        Response: Send + 'static,
        Handle: FnOnce(Request) -> Answer,
        Answer: Future<Output = Result<Response, Status>> + Send + 'static,
    {
        type Response = Response;
        type Future =
            Pin<Box<dyn Future<Output = Result<tonic::Response<Response>, Status>> + Send>>;
        fn call(&mut self, request: tonic::Request<Request>) -> Self::Future {
            let handle = self.0.take().expect("a unary service answers once");
            let answer = handle(request.into_inner());
            Box::pin(async move { answer.await.map(tonic::Response::new) })
        }
    }
    Box::pin(async move {
        Grpc::new(ProstCodec::<Response, Request>::default())
            .unary(Once(Some(handle)), request)
            .await
    })
}

impl NamedService for Gateway {
    const NAME: &'static str = "openshell.v1.OpenShell";
}

impl tower::Service<http::Request<Body>> for Gateway {
    type Response = http::Response<Body>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Infallible>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: http::Request<Body>) -> Self::Future {
        let method = request
            .uri()
            .path()
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        let response = self.handle(&method, request);
        Box::pin(async move { Ok(response.await) })
    }
}
