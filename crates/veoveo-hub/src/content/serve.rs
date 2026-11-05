//! `{token}.{content domain}` on the Apps listener: each request reads its token, admits the
//! person again, and asks the owner whether they may read the item, then answers by its path.
//! `{slug}-{token}.{content domain}` names one of a server's links, which the server checks on
//! every request, so the Hub holds nothing about it.
use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use percent_encoding::percent_decode_str;
use tower::ServiceExt as _;
use tower_http::services::ServeDir;
use veoveo_hub_sdk::{
    ServerSlug,
    registration::{ContentLinkLabel, PublicRoutePrefix, ViewerDeclaration},
};

use veoveo_types::PrincipalId;

use super::{ContentDomain, pages, token, view::canonical_segment};
use crate::{
    HubCatalog,
    apps::{bridge::document_navigation, pages::message},
    computers::{family_session, session_digest},
    problem::Problem,
    registration::{RegisteredServer, RegistrationSnapshot},
    store::ContentToken,
};

const RESERVED: &str = "/_veoveo/";
const VIEWER_FILES: &str = "/_veoveo/viewer/";
const SERVER_VIEWERS: &str = "/_veoveo/viewers/";
const VIEWER_PAGE: &str = "/_veoveo/view/";
const FILE_CACHE: &str = "private, no-transform";
/// `file-viewer`'s files are published code, and an origin lives at most this long.
const VIEWER_CACHE: &str = "private, max-age=86400";

/// What a browser sends that would name its origin, and so the token, to a server.
const BROWSER_HEADERS: [&str; 8] = [
    "authorization",
    "referer",
    "origin",
    "forwarded",
    "x-forwarded-for",
    "x-forwarded-host",
    "x-forwarded-proto",
    "x-forwarded-port",
];

/// What opens a file.
pub(super) enum Viewer {
    /// A server's declared viewer.
    Declared(ServerSlug),
    /// The file itself: HTML, or a folder's listing.
    Itself,
    /// `file-viewer`.
    File,
}

/// The decoded last segment of a raw path, the file's name; none for a folder.
pub(super) fn file_name(path: &str) -> Option<String> {
    let last = path.rsplit('/').next().filter(|last| !last.is_empty())?;
    percent_decode_str(last)
        .decode_utf8()
        .ok()
        .map(|name| name.into_owned())
}

/// The lowercase extension of a raw path.
fn extension(path: &str) -> Option<String> {
    let name = file_name(path)?;
    let (_, extension) = name.rsplit_once('.')?;
    Some(extension.to_ascii_lowercase())
}

/// The deployment among `servers` that serves `slug`'s viewer, and the viewer it declares.
fn declared<'a>(
    servers: &'a RegistrationSnapshot,
    slug: &ServerSlug,
) -> Option<(&'a RegisteredServer, &'a ViewerDeclaration)> {
    servers
        .iter()
        .filter(|server| &server.slug == slug && server.link().is_none())
        .find_map(|server| Some((server, server.surfaces.as_ref()?.viewer.as_ref()?)))
}

/// What opens the file at the raw `path` for a person who reaches `servers`: a declared viewer,
/// the first by slug; HTML and folders as themselves; otherwise `file-viewer`.
pub(super) fn viewer_for(servers: &RegistrationSnapshot, path: &str) -> Viewer {
    if file_name(path).is_none() {
        return Viewer::Itself;
    }
    let Some(extension) = extension(path) else {
        return Viewer::File;
    };
    let declared = servers
        .iter()
        .filter(|server| {
            declared(servers, &server.slug).is_some_and(|(_, viewer)| {
                viewer
                    .extensions
                    .iter()
                    .any(|taken| taken.as_str() == extension)
            })
        })
        .map(|server| &server.slug)
        .min();
    match declared {
        Some(slug) => Viewer::Declared(slug.clone()),
        None if matches!(extension.as_str(), "html" | "htm" | "xhtml") => Viewer::Itself,
        None => Viewer::File,
    }
}

/// The viewer page of the file at the raw path `rest`.
fn viewer_page(servers: &RegistrationSnapshot, rest: &str) -> Response {
    match viewer_for(servers, rest) {
        Viewer::Declared(slug) => {
            let file = format!("/{rest}");
            let file =
                percent_encoding::utf8_percent_encode(&file, percent_encoding::NON_ALPHANUMERIC);
            see_other(&format!("{SERVER_VIEWERS}{slug}/?file={file}"))
        }
        Viewer::Itself => see_other(&format!("/{rest}")),
        Viewer::File => match file_name(rest) {
            Some(name) => pages::viewer_page(&format!("/{rest}"), &name),
            None => Problem::not_found().into_response(),
        },
    }
}

impl ContentDomain {
    /// The token a request's name holds, when its name lies under the domain, whatever it is.
    pub fn claims(&self, headers: &HeaderMap) -> Option<String> {
        self.origin.label(headers)
    }

    /// The deployment `slug` routes to.
    pub(super) fn routed<'a>(
        &self,
        catalog: &'a HubCatalog,
        slug: &ServerSlug,
    ) -> Option<&'a RegisteredServer> {
        let registrations = catalog.registrations();
        let routed = registrations.namespaces().get(slug)?.routed;
        registrations.deployment(&routed)
    }

    /// The deployment that serves `slug`'s links, and their route.
    fn links<'a>(
        &self,
        catalog: &'a HubCatalog,
        slug: &ServerSlug,
    ) -> Option<(&'a RegisteredServer, &'a PublicRoutePrefix)> {
        let server = self.routed(catalog, slug)?;
        let links = server.surfaces.as_ref()?.content.as_ref()?.links.as_ref()?;
        Some((server, links))
    }

    /// The servers `person` reaches now, whose viewers open their files; those of the base branch
    /// for no person, as on a share link's origin.
    pub(super) async fn viewers_for(
        &self,
        catalog: &HubCatalog,
        person: Option<&PrincipalId>,
    ) -> Result<RegistrationSnapshot, Problem> {
        crate::execution::routed_for(&self.hub, catalog, person)
            .await
            .map_err(|error| {
                tracing::error!("failed to read a person's selection: {error:#}");
                Problem::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "content-unavailable",
                    "This file is unavailable",
                )
            })
    }

    /// One request on the origin of `name`, what stood before the domain in its host.
    pub async fn serve(&self, name: &str, mut request: Request) -> Response {
        // This origin holds no session; whatever a file set on it stays with the browser.
        request.headers_mut().remove(header::COOKIE);
        let mut response = self.answer(name, request).await;
        let headers = response.headers_mut();
        // A separate policy, so that a viewer's or the page's own stays whole.
        if let Ok(policy) =
            HeaderValue::from_str(&format!("frame-ancestors {} 'self'", self.issuer))
        {
            headers.append(header::CONTENT_SECURITY_POLICY, policy);
        }
        headers.insert(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        );
        headers.insert(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        );
        // A hostile file or a server sets no cookie that anything here would read.
        headers.remove(header::SET_COOKIE);
        // A proxy that may transform HTML adds to it: Cloudflare, its analytics beacon, which
        // would be a request leaving this origin.
        let html = headers
            .get(header::CONTENT_TYPE)
            .is_some_and(|value| value.as_bytes().starts_with(b"text/html"));
        let kept = headers
            .get_all(header::CACHE_CONTROL)
            .iter()
            .any(|value| value.as_bytes().ends_with(b"no-transform"));
        if html && !kept {
            headers.append(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-transform"),
            );
        }
        response
    }

    async fn answer(&self, name: &str, request: Request) -> Response {
        let link = ContentLinkLabel::parse(name);
        if !token::well_formed(name) && link.is_none() {
            return Problem::not_found().into_response();
        }
        if !matches!(*request.method(), Method::GET | Method::HEAD) {
            return Problem::new(
                StatusCode::METHOD_NOT_ALLOWED,
                "content-read-only",
                "Content is read-only",
            )
            .into_response();
        }
        if let Some(link) = link {
            return self.link(&link, request).await;
        }
        let store = self.hub.hub_store();
        let row = match store.content_token(&session_digest(name)).await {
            Ok(Some(row)) => row,
            Ok(None) => return Problem::not_found().into_response(),
            Err(error) => {
                tracing::error!("failed to read a content token: {error:#}");
                return unavailable();
            }
        };
        if row.expires_at <= Utc::now() {
            return self.ended(&row, &request);
        }
        let catalog = self.catalog.current();
        let session = match family_session(&self.hub, &catalog, &row.computer, &row.family).await {
            Ok(Some(session)) => session,
            Ok(None) => return self.ended(&row, &request),
            Err(error) => {
                tracing::error!("failed to admit a content token's session: {error:#}");
                return unavailable();
            }
        };
        let Some(owner) = self.owner(&catalog, &row.owner) else {
            return Problem::not_found().into_response();
        };
        let path = request.uri().path().to_owned();
        if path.starts_with(RESERVED) {
            if let Err(problem) = owner.admit(&catalog, &session, &row.item).await {
                return problem.into_response();
            }
            return self
                .reserved(&catalog, Some(&session.principal().id), &path, request)
                .await;
        }
        let file = path.strip_prefix('/').unwrap_or(&path);
        let answer = owner
            .read(
                &catalog,
                &session,
                &row.item,
                file,
                request.method(),
                request.headers(),
            )
            .await;
        file_answer(answer.unwrap_or_else(IntoResponse::into_response), file)
    }

    /// One request on the origin of a server's link: its check, the Hub's own paths, and
    /// otherwise the server's file, read through the public route the server declares for its
    /// links. A link names nobody, so every refusal is the same 404.
    async fn link(&self, link: &ContentLinkLabel, request: Request) -> Response {
        let catalog = self.catalog.current();
        let Some((server, links)) = self.links(&catalog, link.server()) else {
            return Problem::not_found().into_response();
        };
        let token = link.token().as_str();
        let path = request.uri().path().to_owned();
        if path.starts_with(RESERVED) {
            let check = Request::head("/")
                .body(Body::empty())
                .expect("a fixed request");
            let answer = self
                .public_routes
                .reach(
                    &catalog,
                    server.deployment,
                    &format!("{links}{token}"),
                    check,
                )
                .await;
            return match answer.status() {
                status if status.is_success() => {
                    self.reserved(&catalog, None, &path, request).await
                }
                StatusCode::TOO_MANY_REQUESTS => answer,
                StatusCode::NOT_FOUND => Problem::not_found().into_response(),
                status => {
                    tracing::warn!(%status, "a server did not answer for its link");
                    unavailable()
                }
            };
        }
        let (mut parts, body) = request.into_parts();
        for name in BROWSER_HEADERS {
            parts.headers.remove(name);
        }
        let answer = self
            .public_routes
            .reach(
                &catalog,
                server.deployment,
                &format!("{links}{token}{path}"),
                Request::from_parts(parts, body),
            )
            .await;
        file_answer(answer, path.strip_prefix('/').unwrap_or(&path))
    }

    /// What the Hub answers itself on an item's origin, once the item is admitted.
    async fn reserved(
        &self,
        catalog: &HubCatalog,
        person: Option<&PrincipalId>,
        path: &str,
        request: Request,
    ) -> Response {
        if let Some(rest) = path.strip_prefix(VIEWER_FILES) {
            return self.viewer_file(rest, request).await;
        }
        if !path.starts_with(SERVER_VIEWERS) && !path.starts_with(VIEWER_PAGE) {
            return Problem::not_found().into_response();
        }
        let servers = match self.viewers_for(catalog, person).await {
            Ok(servers) => servers,
            Err(problem) => return problem.into_response(),
        };
        if let Some(rest) = path.strip_prefix(SERVER_VIEWERS) {
            return self.server_viewer(catalog, &servers, rest, request).await;
        }
        match path.strip_prefix(VIEWER_PAGE) {
            Some(rest) => viewer_page(&servers, rest),
            None => Problem::not_found().into_response(),
        }
    }

    /// `file-viewer`'s file at the raw path `rest`, from its mounted image.
    async fn viewer_file(&self, rest: &str, request: Request) -> Response {
        let (mut parts, body) = request.into_parts();
        let Ok(uri) = format!("/{rest}").parse::<Uri>() else {
            return Problem::not_found().into_response();
        };
        parts.uri = uri;
        let files = ServeDir::new(&self.viewer_files).append_index_html_on_directories(false);
        let answer = files.oneshot(Request::from_parts(parts, body)).await;
        let mut response = match answer {
            Ok(response) => response.map(Body::new),
            Err(never) => match never {},
        };
        if response.status().is_success() || response.status() == StatusCode::NOT_MODIFIED {
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static(VIEWER_CACHE),
            );
        }
        response
    }

    /// `{slug}/{rest}`: `rest` under the viewer route `slug` declares, without anything of the
    /// browser's that names the token.
    async fn server_viewer(
        &self,
        catalog: &HubCatalog,
        servers: &RegistrationSnapshot,
        rest: &str,
        request: Request,
    ) -> Response {
        let (slug, rest) = rest.split_once('/').unwrap_or((rest, ""));
        let Ok(slug) = ServerSlug::parse(slug) else {
            return Problem::not_found().into_response();
        };
        let Some((server, viewer)) = declared(servers, &slug) else {
            return Problem::not_found().into_response();
        };
        let (mut parts, body) = request.into_parts();
        for name in BROWSER_HEADERS {
            parts.headers.remove(name);
        }
        self.public_routes
            .reach(
                catalog,
                server.deployment,
                &format!("{}{rest}", viewer.route),
                Request::from_parts(parts, body),
            )
            .await
    }

    /// The answer of a token that expired or whose session ended. A page load goes to `/view/`,
    /// which signs in if needed and mints a new one; in the Hub's frame, which may not show the
    /// Hub, it says so and the Hub's page reloads itself.
    fn ended(&self, row: &ContentToken, request: &Request) -> Response {
        let headers = request.headers();
        if document_navigation(headers) {
            let path = request.uri().path();
            let file = path
                .strip_prefix(VIEWER_PAGE)
                .unwrap_or_else(|| path.strip_prefix('/').unwrap_or(path));
            let file = if path.starts_with(RESERVED) && !path.starts_with(VIEWER_PAGE) {
                ""
            } else {
                file
            };
            let spelled = file
                .split('/')
                .map(canonical_segment)
                .collect::<Vec<_>>()
                .join("/");
            return see_other(&format!(
                "{}/view/{}/{}/{spelled}",
                self.issuer, row.owner, row.item
            ));
        }
        if headers
            .get("sec-fetch-mode")
            .is_some_and(|value| value == "navigate")
        {
            let mut page = message(
                StatusCode::UNAUTHORIZED,
                "This preview expired",
                "Reload the page to open it again.",
            );
            // `message`'s policy forbids every frame, and this one is the Hub's.
            page.headers_mut().remove(header::CONTENT_SECURITY_POLICY);
            return page;
        }
        Problem::new(
            StatusCode::UNAUTHORIZED,
            "content-token-expired",
            "This preview expired",
        )
        .detail("Reload the page to open it again.")
        .into_response()
    }
}

/// An owner's answer of the file at the raw `path`, as the origin sends it.
fn file_answer(mut response: Response, path: &str) -> Response {
    name_type(&mut response, path);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(FILE_CACHE));
    response
}

/// A file its owner calls `application/octet-stream` takes its type from its extension.
fn name_type(response: &mut Response, path: &str) {
    if !matches!(
        response.status(),
        StatusCode::OK | StatusCode::PARTIAL_CONTENT
    ) {
        return;
    }
    let headers = response.headers_mut();
    if headers
        .get(header::CONTENT_TYPE)
        .is_some_and(|value| value == "application/octet-stream")
        && let Some(name) = file_name(path)
        && let Some(named) = mime_guess::from_path(name).first()
        && let Ok(value) = HeaderValue::from_str(named.as_ref())
    {
        headers.insert(header::CONTENT_TYPE, value);
    }
}

fn see_other(location: &str) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    match HeaderValue::from_str(location) {
        Ok(location) => {
            let headers = response.headers_mut();
            headers.insert(header::LOCATION, location);
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Err(_) => Problem::internal().into_response(),
    }
}

fn unavailable() -> Response {
    Problem::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "content-unavailable",
        "This file is unavailable",
    )
    .detail("The Hub could not open this file. Try again later.")
    .into_response()
}
