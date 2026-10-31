//! Each App's host page and bridge, for the person's browser session.
use std::sync::Arc;

use anyhow::Context as _;
use axum::{
    Json, Router,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use rmcp::model::{Resource, Tool};
use serde_json::{Map, Value, json};
use veoveo_hub_sdk::PrincipalKind;
use veoveo_types::InvocationProvenance;

use super::{
    admission::{self, INTERNAL_ERROR, Method, Refusal, RequestId, ViewRequest},
    csp::AppPolicy,
    listen::{self, ListenLimits, Listens},
    mcp::{Answer, Failure, Listened, PersonMcp},
    origins::AppOrigins,
    pages,
    route::AppView,
};
use crate::{HubCatalog, HubCatalogHandle, HubState, problem::Problem};

/// The largest request a view sends.
const MAX_REQUEST_BYTES: usize = 256 * 1024;
/// How long the bearer of one bridge request lives, at most.
const BEARER_SECONDS: u32 = 60;
/// How long the bearer of one listen lives, at most: the longest a listen's forwarded
/// token lives.
const LISTEN_BEARER_SECONDS: u32 = 15 * 60;

/// The Apps host on the issuer's origin: host pages, the host script and bridges.
#[derive(Clone)]
pub struct AppsService {
    catalog: HubCatalogHandle,
    hub: HubState,
    origins: Arc<AppOrigins>,
    host: HeaderValue,
    mcp: Router,
    limits: ListenLimits,
    listens: Arc<Listens>,
}

impl AppsService {
    /// The host for the Hub whose `/mcp/{profile}` routes `mcp` serves, with their own
    /// authentication, at the `Host` authority `host`, with live resources bounded by
    /// `limits`.
    pub fn new(
        catalog: HubCatalogHandle,
        hub: HubState,
        origins: AppOrigins,
        host: &str,
        mcp: Router,
        limits: ListenLimits,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            catalog,
            hub,
            origins: Arc::new(origins),
            host: HeaderValue::from_str(host).context("the issuer's host authority")?,
            mcp,
            limits,
            listens: Arc::default(),
        })
    }

    pub fn origins(&self) -> &AppOrigins {
        &self.origins
    }

    /// `GET /apps/host-{digest}.js`, and `GET` and `POST /apps/{slug}/{page}`.
    pub fn router(self) -> Router {
        Router::new()
            .route(
                pages::host_script_path(),
                get(|| async { pages::host_script() }),
            )
            .route("/apps/{slug}/{*page}", get(open).post(bridge))
            .with_state(self)
    }

    /// The person's MCP client for this session, with a bearer of its refresh family
    /// that lives at most `lifetime` seconds.
    async fn person(
        &self,
        catalog: &HubCatalog,
        config: &crate::control_plane::Computers,
        session: &crate::computers::BrowserSession,
        observation: crate::request_observation::RequestObservation,
        lifetime: u32,
    ) -> anyhow::Result<PersonMcp> {
        let token = family_bearer(&self.hub, catalog, config, session, lifetime).await?;
        Ok(PersonMcp {
            router: self.mcp.clone(),
            host: self.host.clone(),
            profile: config.profile.clone(),
            bearer: HeaderValue::from_str(&format!("Bearer {}", token.access_token))?,
            expires_at: token.expires_at,
            observation,
        })
    }
}

/// An access token of `session`'s refresh family under the Computers profile that lives
/// at most `lifetime` seconds and never past the family.
pub(super) async fn family_bearer(
    hub: &crate::HubState,
    catalog: &HubCatalog,
    config: &crate::control_plane::Computers,
    session: &crate::computers::BrowserSession,
    lifetime: u32,
) -> anyhow::Result<crate::issuance::IssuedAccessToken> {
    let family = hub
        .hub_store()
        .refresh_family(session.family())
        .await?
        .context("the session's refresh family is gone")?;
    let profile = catalog
        .profile(&config.profile)
        .context("the Computers profile is unavailable")?;
    let server = catalog
        .authorization_server(&profile.authorization_server)
        .context("the Computers authorization server is unavailable")?;
    let principal = &family.principal.principal;
    crate::issuance::issue_access_token(
        catalog,
        server,
        &family.protected_resource,
        &principal.subject,
        &family.oauth_client_id,
        PrincipalKind::User,
        Some(principal),
        Some(&family.principal.display_name),
        None,
        crate::issuance::AccessTokenInvocation {
            lifetime: Some(crate::token_profile::TokenLifetime::from_seconds(
                std::num::NonZeroU32::new(lifetime).context("a bearer lives a while")?,
            )),
            not_after: Some(family.expires_at),
            purpose: None,
            extensions: Default::default(),
            session_family: Some(family.id.clone()),
            work_context: family.work_context.clone(),
            provenance: InvocationProvenance::Direct {
                initiator: principal.id.clone(),
            },
        },
        principal.id.clone(),
        &family.scopes,
    )
    .await
}

/// A browser's page load: a GET the browser marks as a navigation, of a document when it
/// names the destination.
pub(super) fn document_navigation(headers: &HeaderMap) -> bool {
    headers
        .get("sec-fetch-mode")
        .is_some_and(|value| value == "navigate")
        && headers
            .get("sec-fetch-dest")
            .is_none_or(|value| value == "document")
}

fn same_origin(headers: &HeaderMap, origin: &str) -> bool {
    let mut values = headers.get_all(header::ORIGIN).iter();
    matches!((values.next(), values.next()), (Some(value), None) if value.as_bytes() == origin.as_bytes())
}

fn sign_in_required() -> Response {
    Problem::new(
        StatusCode::UNAUTHORIZED,
        "apps-sign-in-required",
        "Sign-in required",
    )
    .detail("Open the App's page to sign in.")
    .into_response()
}

fn unavailable(error: &dyn std::fmt::Display) -> Response {
    tracing::warn!(%error, "App host unavailable");
    Problem::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "apps-unavailable",
        "Apps unavailable",
    )
    .into_response()
}

/// `GET /apps/{slug}/{page}`: the App's host page, once the App is in the catalog the
/// person sees and its declared policy is admitted.
async fn open(State(apps): State<AppsService>, request: Request) -> Response {
    let Some(observation) = request_observation(&request) else {
        return unavailable(&"the request carries no correlation");
    };
    let Some(view) = AppView::from_path(request.uri().path()) else {
        return pages::message(
            StatusCode::NOT_FOUND,
            "No App here",
            "This address names no App.",
        );
    };
    let catalog = apps.catalog.current();
    if catalog
        .runtime
        .as_ref()
        .and_then(|runtime| runtime.computers.as_ref())
        .is_none()
    {
        return pages::message(
            StatusCode::NOT_FOUND,
            "No App here",
            "This installation signs no one in to Apps.",
        );
    }
    let headers = request.headers();
    let (config, session) = match crate::computers::app_session(&apps.hub, &catalog, headers).await
    {
        Ok(Some(admitted)) => admitted,
        Ok(None) if document_navigation(headers) => {
            return crate::computers::sign_in_for_app(&view)
                .unwrap_or_else(|error| unavailable(&error));
        }
        Ok(None) => return sign_in_required(),
        Err(error) => return unavailable(&error),
    };
    let person = match apps
        .person(
            &catalog,
            &config,
            &session,
            observation.clone(),
            BEARER_SECONDS,
        )
        .await
    {
        Ok(person) => person,
        Err(error) => return unavailable(&error),
    };
    let resources: Vec<Resource> = match person.list("resources/list", "resources").await {
        Ok(resources) => resources,
        Err(Failure::Unauthenticated) => {
            return crate::computers::sign_in_for_app(&view)
                .unwrap_or_else(|error| unavailable(&error));
        }
        Err(error) => return unavailable(&error),
    };
    let Ok(app) = admission::find_app(&view, &resources) else {
        return pages::message(
            StatusCode::NOT_FOUND,
            "App not available",
            &format!(
                "{} is not an App you can open now: you have no access to it, or its server is not serving.",
                view.uri()
            ),
        );
    };
    let policy = match AppPolicy::admit(app, view.server(), &apps.origins) {
        Ok(policy) => policy,
        Err(error) => {
            tracing::info!(app = view.uri(), %error, "App refused for its declared policy");
            return pages::message(StatusCode::BAD_GATEWAY, "App refused", &error.to_string());
        }
    };
    let title = app.title.as_deref().unwrap_or(&app.name);
    let app_origin = super::server_origin::declares_origin(&catalog, view.server())
        .then(|| apps.origins.app_origin(view.server()));
    pages::host_page(&view, title, &policy, &apps.origins, app_origin)
}

fn response(id: &RequestId, answer: Answer) -> Response {
    let body = match answer {
        Answer::Result(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Answer::Error(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
    };
    let mut response = Json(body).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

fn error(code: i64, message: impl Into<String>) -> Answer {
    Answer::Error(json!({"code": code, "message": message.into()}))
}

/// `POST /apps/{slug}/{page}`: one JSON-RPC request of the App's view, admitted against
/// the catalogs the person sees now and run through the Hub's MCP endpoint as them.
async fn bridge(State(apps): State<AppsService>, request: Request) -> Response {
    let Some(observation) = request_observation(&request) else {
        return unavailable(&"the request carries no correlation");
    };
    let Some(view) = AppView::from_path(request.uri().path()) else {
        return Problem::not_found().into_response();
    };
    if !same_origin(request.headers(), apps.origins.host()) {
        return Problem::new(
            StatusCode::FORBIDDEN,
            "apps-origin-refused",
            "Origin refused",
        )
        .detail("An App's bridge answers only its host page's origin.")
        .into_response();
    }
    let catalog = apps.catalog.current();
    let (config, session) =
        match crate::computers::app_session(&apps.hub, &catalog, request.headers()).await {
            Ok(Some(admitted)) => admitted,
            Ok(None) => return sign_in_required(),
            Err(error) => return unavailable(&error),
        };
    // An open listen reads the session again from these cookies alone.
    let mut cookies = HeaderMap::new();
    for value in request.headers().get_all(header::COOKIE) {
        cookies.append(header::COOKIE, value.clone());
    }
    let bytes = match axum::body::to_bytes(request.into_body(), MAX_REQUEST_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return Problem::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "apps-request-too-large",
                "Request too large",
            )
            .into_response();
        }
    };
    let Ok(view_request) = serde_json::from_slice::<ViewRequest>(&bytes) else {
        return Problem::new(
            StatusCode::BAD_REQUEST,
            "apps-request-invalid",
            "Invalid request",
        )
        .detail("The body is not one JSON-RPC 2.0 request.")
        .into_response();
    };
    let id = view_request.id.clone();
    if Method::parse(&view_request.method) == Some(Method::SubscriptionsListen) {
        let session = Admitted {
            catalog,
            config,
            session,
            cookies,
        };
        return listen(&apps, &view, view_request, session, observation).await;
    }
    let person = match apps
        .person(
            &catalog,
            &config,
            &session,
            observation.clone(),
            BEARER_SECONDS,
        )
        .await
    {
        Ok(person) => person,
        Err(error) => return unavailable(&error),
    };
    match serve(&view, &person, view_request).await {
        Ok(answer) => response(&id, answer),
        Err(Failure::Unauthenticated) => sign_in_required(),
        Err(Failure::Unavailable(cause)) => {
            tracing::warn!(app = view.uri(), %cause, "App request unavailable");
            response(
                &id,
                error(INTERNAL_ERROR, "The Hub could not complete this request."),
            )
        }
    }
}

/// The browser session a bridge request was admitted with.
struct Admitted {
    catalog: Arc<HubCatalog>,
    config: Arc<crate::control_plane::Computers>,
    session: crate::computers::BrowserSession,
    /// The request's cookies, from which an open listen reads the session again.
    cookies: HeaderMap,
}

/// A view's `subscriptions/listen`: admitted against the person's catalog, within the
/// session's ceiling, sent as the person with the view's own `id`, and answered with the
/// endpoint's event stream, relayed unchanged.
async fn listen(
    apps: &AppsService,
    view: &AppView,
    request: ViewRequest,
    admitted: Admitted,
    observation: crate::request_observation::RequestObservation,
) -> Response {
    let Admitted {
        catalog,
        config,
        session,
        cookies,
    } = admitted;
    let id = request.id;
    let params = request.params.unwrap_or_default();
    let person = match apps
        .person(
            &catalog,
            &config,
            &session,
            observation.clone(),
            BEARER_SECONDS,
        )
        .await
    {
        Ok(person) => person,
        Err(error) => return unavailable(&error),
    };
    let unanswered = |cause: String| {
        tracing::warn!(app = view.uri(), %cause, "App listen unavailable");
        response(
            &id,
            error(INTERNAL_ERROR, "The Hub could not complete this request."),
        )
    };
    let resources: Vec<Resource> = match person.list("resources/list", "resources").await {
        Ok(resources) => resources,
        Err(Failure::Unauthenticated) => return sign_in_required(),
        Err(Failure::Unavailable(cause)) => return unanswered(cause),
    };
    if let Err(refusal) =
        admission::find_app(view, &resources).and_then(|_| admission::admit_listen(view, &params))
    {
        tracing::info!(app = view.uri(), method = "subscriptions/listen", %refusal, "App request refused");
        return response(&id, error(refusal.code(), refusal.to_string()));
    }
    let Some(permit) = apps
        .listens
        .acquire(session.family(), apps.limits.per_session)
    else {
        tracing::info!(
            app = view.uri(),
            limit = apps.limits.per_session.get(),
            "App listen refused at the session's ceiling"
        );
        return Problem::new(
            StatusCode::TOO_MANY_REQUESTS,
            "apps-listen-limit",
            "Too many live listens",
        )
        .detail(format!(
            "This browser session holds {} open listens, the most it may.",
            apps.limits.per_session
        ))
        .into_response();
    };
    let wire_id = match serde_json::to_value(&id) {
        Ok(wire_id) => wire_id,
        Err(cause) => return unanswered(cause.to_string()),
    };
    // The listen alone gets the bearer that lives as long as a listen's forwarded token.
    let person = match apps
        .person(
            &catalog,
            &config,
            &session,
            observation,
            LISTEN_BEARER_SECONDS,
        )
        .await
    {
        Ok(person) => person,
        Err(error) => return unavailable(&error),
    };
    match person.listen(&wire_id, params).await {
        Ok(Listened::Answer(answer)) => response(&id, answer),
        Ok(Listened::Stream(body)) => {
            let hub = apps.hub.clone();
            let handle = apps.catalog.clone();
            let family = session.family().clone();
            let cookies = Arc::new(cookies);
            listen::relay(listen::Relay {
                id: wire_id,
                app: view.uri().to_owned(),
                body,
                permit,
                expires_at: person.expires_at,
                recheck: apps.limits.recheck,
                admitted: Arc::new(move || {
                    let (hub, handle, family, cookies) =
                        (hub.clone(), handle.clone(), family.clone(), cookies.clone());
                    Box::pin(async move {
                        matches!(
                            tokio::time::timeout(
                                crate::computers::ADMISSION_READ_TIMEOUT,
                                crate::computers::app_session(&hub, &handle.current(), &cookies),
                            )
                            .await,
                            Ok(Ok(Some((_, session)))) if session.family() == &family
                        )
                    })
                }),
            })
        }
        Err(Failure::Unauthenticated) => sign_in_required(),
        Err(Failure::Unavailable(cause)) => unanswered(cause),
    }
}

/// Admit one view request and send it as the person, or answer it.
async fn serve(
    view: &AppView,
    person: &PersonMcp,
    request: ViewRequest,
) -> Result<Answer, Failure> {
    let Some(method) = admission::Method::parse(&request.method) else {
        let refusal = Refusal::Method(request.method);
        return Ok(error(refusal.code(), refusal.to_string()));
    };
    let mut params = request.params.unwrap_or_default();
    let (resources, tools) = if method == Method::ToolsCall {
        let (resources, tools) = tokio::join!(
            person.list::<Resource>("resources/list", "resources"),
            person.list::<Tool>("tools/list", "tools"),
        );
        (resources?, tools?)
    } else {
        (
            person
                .list::<Resource>("resources/list", "resources")
                .await?,
            Vec::new(),
        )
    };
    let admitted = admission::find_app(view, &resources).and_then(|app| match method {
        Method::Ping => Ok(None),
        Method::ToolsCall => admission::admit_tool(app, &tools, &mut params).map(Some),
        Method::ResourcesRead => admission::admit_read(view, &params).map(|()| None),
        Method::TasksGet | Method::TasksUpdate | Method::TasksCancel => {
            admission::admit_task(&params).map(|()| None)
        }
        // The bridge answers a listen with its stream before it comes here.
        Method::SubscriptionsListen => Err(Refusal::Listen),
    });
    let tool = match admitted {
        Ok(tool) => tool,
        Err(refusal) => {
            tracing::info!(app = view.uri(), method = method.name(), %refusal, "App request refused");
            return Ok(error(refusal.code(), refusal.to_string()));
        }
    };
    if method == Method::Ping {
        return Ok(Answer::Result(Value::Object(Map::new())));
    }
    person.request(method.name(), params, tool.as_ref()).await
}

/// The correlation the Hub's request observation gave this browser request; the
/// in-process MCP request carries it, so its records name the browser request.
fn request_observation(
    request: &Request,
) -> Option<crate::request_observation::RequestObservation> {
    request
        .extensions()
        .get::<crate::request_observation::RequestObservation>()
        .cloned()
}
