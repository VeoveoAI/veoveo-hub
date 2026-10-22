//! A fake OpenShell gateway: an in-process gRPC server that answers the requests the
//! factory sends from sandboxes and providers it keeps in memory, and records every
//! request that changes them. Like the real gateway, it stops a sandbox that is ready,
//! stopping, failed or in an automatic restart, and resets its restart count, answers a
//! stop of a stopped or completed sandbox without change and refuses one still
//! provisioning or starting for the first time; it starts a stopped, completed, failed
//! or starting sandbox at once and answers a start of a ready one without change. Any
//! other method is unimplemented. It serves plain gRPC to the test and TLS to the Hub
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
    UpdateConfig(p::UpdateConfigRequest),
    StopSandbox(p::StopSandboxRequest),
    StartSandbox(p::StartSandboxRequest),
    DeleteSandbox(p::DeleteSandboxRequest),
}

#[derive(Default)]
struct State {
    calls: Vec<Call>,
    /// By workspace and name.
    sandboxes: BTreeMap<(String, String), p::Sandbox>,
    providers: BTreeMap<(String, String), p::Provider>,
    /// The refusal the next request of a method answers instead, by method.
    refusals: BTreeMap<String, Status>,
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
                let sandbox = p::Sandbox {
                    metadata: Some(p::ObjectMeta {
                        name: key.1.clone(),
                        workspace: key.0.clone(),
                        ..Default::default()
                    }),
                    spec: request.spec.clone(),
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
            "UpdateConfig" => unary(request, move |request: p::UpdateConfigRequest| {
                let mut state = state.lock().unwrap();
                let key = (
                    workspace(request.workspace_scope.as_ref())?,
                    request.sandbox.clone(),
                );
                if !state.sandboxes.contains_key(&key) {
                    return Err(Status::not_found("sandbox not found"));
                }
                state.calls.push(Call::UpdateConfig(request));
                Ok(p::UpdateConfigResponse::default())
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
