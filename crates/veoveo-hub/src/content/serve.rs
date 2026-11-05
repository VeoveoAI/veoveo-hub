//! `{token}.{content domain}` on the Apps listener: each request reads its token, admits the
//! person again, and asks the owner whether they may read the item, then answers by its path.
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
use veoveo_hub_sdk::{ServerSlug, registration::ViewerDeclaration};

use super::{ContentDomain, pages, token, view::canonical_segment};
use crate::{
    HubCatalog,
    apps::{bridge::document_navigation, pages::message},
    computers::{family_session, session_digest},
    problem::Problem,
    registration::RegisteredServer,
    store::ContentToken,
};

const RESERVED: &str = "/_veoveo/";
const VIEWER_FILES: &str = "/_veoveo/viewer/";
const SERVER_VIEWERS: &str = "/_veoveo/viewers/";
const VIEWER_PAGE: &str = "/_veoveo/view/";
const FILE_CACHE: &str = "private, no-transform";
/// `file-viewer`'s files are published code, and an origin lives at most this long.
const VIEWER_CACHE: &str = "private, max-age=86400";

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

impl ContentDomain {
    /// The token a request's name holds, when its name lies under the domain, whatever it is.
    pub fn claims(&self, headers: &HeaderMap) -> Option<String> {
        self.origin.label(headers)
    }

    /// The deployment that serves `slug`'s viewer, and the viewer it declares.
    fn declared<'a>(
        &self,
        catalog: &'a HubCatalog,
        slug: &ServerSlug,
    ) -> Option<(&'a RegisteredServer, &'a ViewerDeclaration)> {
        let registrations = catalog.registrations();
        let routed = registrations.namespaces().get(slug)?.routed;
        let server = registrations.deployment(&routed)?;
        let viewer = server.surfaces.as_ref()?.viewer.as_ref()?;
        Some((server, viewer))
    }

    /// What opens the file at the raw `path`: a declared viewer, the first by slug; HTML and
    /// folders as themselves; otherwise `file-viewer`.
    pub(super) fn viewer_for(&self, catalog: &HubCatalog, path: &str) -> Viewer {
        if file_name(path).is_none() {
            return Viewer::Itself;
        }
        let Some(extension) = extension(path) else {
            return Viewer::File;
        };
        let declared = catalog.registrations().namespaces().keys().find(|slug| {
            self.declared(catalog, slug).is_some_and(|(_, viewer)| {
                viewer
                    .extensions
                    .iter()
                    .any(|taken| taken.as_str() == extension)
            })
        });
        match declared {
            Some(slug) => Viewer::Declared(slug.clone()),
            None if matches!(extension.as_str(), "html" | "htm" | "xhtml") => Viewer::Itself,
            None => Viewer::File,
        }
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
        response
    }

    async fn answer(&self, name: &str, request: Request) -> Response {
        if !token::well_formed(name) {
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
        let Some(owner) = self.owners.get(&row.owner) else {
            return Problem::not_found().into_response();
        };
        if let Err(problem) = owner.admit(&catalog, session.principal(), &row.item).await {
            return problem.into_response();
        }
        let path = request.uri().path().to_owned();
        if let Some(rest) = path.strip_prefix(VIEWER_FILES) {
            return self.viewer_file(rest, request).await;
        }
        if let Some(rest) = path.strip_prefix(SERVER_VIEWERS) {
            return self.server_viewer(&catalog, rest, request).await;
        }
        if let Some(rest) = path.strip_prefix(VIEWER_PAGE) {
            return self.viewer_page(&catalog, rest);
        }
        if path.starts_with(RESERVED) {
            return Problem::not_found().into_response();
        }
        let file = path.strip_prefix('/').unwrap_or(&path);
        let answer = owner
            .read(
                &catalog,
                session.principal(),
                &row.item,
                file,
                request.headers(),
            )
            .await;
        let mut response = answer.unwrap_or_else(IntoResponse::into_response);
        name_type(&mut response, file);
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static(FILE_CACHE));
        response
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
    async fn server_viewer(&self, catalog: &HubCatalog, rest: &str, request: Request) -> Response {
        let (slug, rest) = rest.split_once('/').unwrap_or((rest, ""));
        let Ok(slug) = ServerSlug::parse(slug) else {
            return Problem::not_found().into_response();
        };
        let Some((server, viewer)) = self.declared(catalog, &slug) else {
            return Problem::not_found().into_response();
        };
        let (mut parts, body) = request.into_parts();
        for name in [
            "referer",
            "origin",
            "forwarded",
            "x-forwarded-for",
            "x-forwarded-host",
            "x-forwarded-proto",
            "x-forwarded-port",
        ] {
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

    /// The viewer page of the file at the raw path `rest`.
    fn viewer_page(&self, catalog: &HubCatalog, rest: &str) -> Response {
        match self.viewer_for(catalog, rest) {
            Viewer::Declared(slug) => {
                let file = format!("/{rest}");
                let file = percent_encoding::utf8_percent_encode(
                    &file,
                    percent_encoding::NON_ALPHANUMERIC,
                );
                see_other(&format!("{SERVER_VIEWERS}{slug}/?file={file}"))
            }
            Viewer::Itself => see_other(&format!("/{rest}")),
            Viewer::File => match file_name(rest) {
                Some(name) => pages::viewer_page(&format!("/{rest}"), &name),
                None => Problem::not_found().into_response(),
            },
        }
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
