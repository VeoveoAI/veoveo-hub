//! `/view/{owner}/{item}/{path}` on the issuer's origin: the person's token on the item, and the
//! page that frames the file.
use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{TimeDelta, Utc};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, percent_encode};
use serde_json::json;
use veoveo_types::Sha256Digest;

use super::{ContentDomain, Named, pages, serve::Viewer, token};
use crate::{
    apps::{
        bridge::{document_navigation, same_origin},
        pages::message,
    },
    computers::{BrowserSession, app_session, session_cookie, session_digest, sign_in_for_entry},
    problem::Problem,
    store::NewContentToken,
};

/// How long a token lives past its last mint.
const TOKEN_LIFETIME: TimeDelta = TimeDelta::hours(1);
/// How long a token can live at all.
const TOKEN_LIMIT: TimeDelta = TimeDelta::hours(24);
/// How long a row stays past its expiry, so that a stale tab finds its way back.
const EXPIRED_RETENTION: TimeDelta = TimeDelta::days(7);
const VIEW_PREFIX: &str = "/view/";
const MAX_VIEW_PATH: usize = 2048;

/// What stays unescaped in a segment of a canonical path: RFC 3986's unreserved characters.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// A raw path segment as canonically spelled.
pub(super) fn canonical_segment(segment: &str) -> String {
    let bytes = percent_decode_str(segment).collect::<Vec<u8>>();
    percent_encode(&bytes, SEGMENT).to_string()
}

/// `path`, a `/view/` path, as canonically spelled: every segment percent-encoded but its
/// unreserved characters. None for any other path, one holding an empty segment but for a last,
/// or `.` or `..` however spelled, or one too long.
pub fn view_return_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix(VIEW_PREFIX)?;
    let (head, last) = rest.rsplit_once('/').unwrap_or(("", rest));
    let segments = head
        .split('/')
        .filter(|_| !head.is_empty())
        .map(|segment| (segment, false))
        .chain([(last, true)]);
    let mut canonical = String::from(VIEW_PREFIX);
    let mut first = true;
    for (segment, is_last) in segments {
        let spelled = canonical_segment(segment);
        if matches!(spelled.as_str(), "." | "..") || (spelled.is_empty() && !is_last) {
            return None;
        }
        if !first {
            canonical.push('/');
        }
        first = false;
        canonical.push_str(&spelled);
    }
    (canonical.len() <= MAX_VIEW_PATH && canonical.len() > VIEW_PREFIX.len()).then_some(canonical)
}

/// The owner and what the rest of a canonical `/view/` path names.
struct Opened<'a> {
    owner: &'a str,
    named: Named,
}

impl ContentDomain {
    /// `GET /view/...`, `POST /view/...` and the page's script.
    pub fn router(&self) -> Router {
        Router::new()
            .route(
                pages::view_script_path(),
                get(|| async { pages::view_script() }),
            )
            .route("/view/{*rest}", get(open).post(renew))
            .with_state(self.clone())
    }

    fn opened<'a>(&self, canonical: &'a str) -> Option<Opened<'a>> {
        let rest = canonical.strip_prefix(VIEW_PREFIX)?;
        let (owner, rest) = rest.split_once('/')?;
        let catalog = self.catalog.current();
        let named = self.owner(&catalog, owner)?.named(rest)?;
        Some(Opened { owner, named })
    }

    /// The token this browser's `cookie` holds on the item of `session`'s person: the one that
    /// lives, extended, or else a new one.
    async fn mint(
        &self,
        cookie: &str,
        opened: &Opened<'_>,
        session: &BrowserSession,
    ) -> Result<String> {
        let store = self.hub.hub_store();
        let held = session_digest(cookie);
        let now = Utc::now();
        let live = store
            .live_content_token(
                &held,
                opened.owner,
                &opened.named.item,
                session.family(),
                now,
            )
            .await?;
        if let Some(nonce) = live {
            let token = token::derive(cookie, &nonce, opened.owner, &opened.named.item);
            store
                .extend_content_token(&token_digest(&token), now + TOKEN_LIFETIME)
                .await?;
            return Ok(token);
        }
        let nonce = token::nonce()?;
        let token = token::derive(cookie, &nonce, opened.owner, &opened.named.item);
        store
            .insert_content_token(
                NewContentToken {
                    digest: &token_digest(&token),
                    nonce: &nonce,
                    session: &held,
                    owner: opened.owner,
                    item: &opened.named.item,
                    computer: &session.computer().id,
                    family: session.family(),
                    expires_at: now + TOKEN_LIFETIME,
                    hard_expires_at: now + TOKEN_LIMIT,
                },
                now - EXPIRED_RETENTION,
            )
            .await?;
        Ok(token)
    }

    /// The token of the request's person on what `opened` names, or the refusal. A visit is what
    /// opening the page is, and the page's renewals are no more of one.
    async fn token(
        &self,
        opened: &Opened<'_>,
        session: &BrowserSession,
        headers: &HeaderMap,
        visit: bool,
    ) -> Result<String, Problem> {
        let catalog = self.catalog.current();
        let owner = self
            .owner(&catalog, opened.owner)
            .ok_or_else(Problem::not_found)?;
        if visit {
            owner.visit(&catalog, session, &opened.named.item).await?;
        } else {
            owner.admit(&catalog, session, &opened.named.item).await?;
        }
        let cookie = session_cookie(headers).ok_or_else(sign_in_required)?;
        self.mint(&cookie, opened, session).await.map_err(|error| {
            tracing::error!("failed to mint a content token: {error:#}");
            unavailable()
        })
    }
}

fn token_digest(token: &str) -> Sha256Digest {
    session_digest(token)
}

fn sign_in_required() -> Problem {
    Problem::new(
        StatusCode::UNAUTHORIZED,
        "content-sign-in-required",
        "Sign-in required",
    )
    .detail("Open the page to sign in.")
}

fn unavailable() -> Problem {
    Problem::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "content-unavailable",
        "This file is unavailable",
    )
    .detail("The Hub could not open this file. Try again later.")
}

/// A refusal as the page a person sees.
fn refused_page(problem: Problem) -> Response {
    match problem.status() {
        StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED => message(
            StatusCode::FORBIDDEN,
            "You cannot open this file",
            "Your account has no access to it. Its owner can grant you access.",
        ),
        StatusCode::NOT_FOUND => message(
            StatusCode::NOT_FOUND,
            "No such file",
            "This address names nothing here, or nothing you can read.",
        ),
        _ => message(
            StatusCode::SERVICE_UNAVAILABLE,
            "This file is unavailable",
            "The Hub could not open this file. Try again later.",
        ),
    }
}

/// The session of a `/view/` request: `Err` for an answer without one.
async fn session_of(
    domain: &ContentDomain,
    headers: &HeaderMap,
    canonical: &str,
) -> Result<BrowserSession, Box<Response>> {
    let catalog = domain.catalog.current();
    let answer = match app_session(&domain.hub, &catalog, headers).await {
        Ok(Some((_, session))) => return Ok(session),
        Ok(None) if document_navigation(headers) => {
            sign_in_for_entry(canonical).unwrap_or_else(|error| {
                tracing::error!("failed to send a person to sign in: {error:#}");
                Problem::internal().into_response()
            })
        }
        Ok(None) => sign_in_required().into_response(),
        Err(error) => {
            tracing::error!("failed to admit a browser session for a view: {error:#}");
            unavailable().into_response()
        }
    };
    Err(Box::new(answer))
}

async fn open(State(domain): State<ContentDomain>, request: Request) -> Response {
    let (parts, _) = request.into_parts();
    let Some(canonical) = view_return_path(parts.uri.path()) else {
        return refused_page(Problem::not_found());
    };
    let Some(opened) = domain.opened(&canonical) else {
        return refused_page(Problem::not_found());
    };
    let session = match session_of(&domain, &parts.headers, &canonical).await {
        Ok(session) => session,
        Err(answer) => return *answer,
    };
    let token = match domain.token(&opened, &session, &parts.headers, true).await {
        Ok(token) => token,
        Err(problem) => return refused_page(problem),
    };
    let origin = domain.origin.under(&token);
    let path = &opened.named.path;
    let catalog = domain.catalog.current();
    let servers = match domain
        .viewers_for(&catalog, Some(&session.principal().id))
        .await
    {
        Ok(servers) => servers,
        Err(problem) => return refused_page(problem),
    };
    let frame = match super::serve::viewer_for(&servers, path) {
        Viewer::Itself => format!("{origin}/{path}"),
        Viewer::Declared(_) | Viewer::File => format!("{origin}/_veoveo/view/{path}"),
    };
    let src = match parts.uri.query() {
        Some(query) => format!("{frame}?{query}"),
        None => frame,
    };
    let title = super::serve::file_name(path).unwrap_or_else(|| opened.named.item.clone());
    let frames = domain.origin.under("*");
    pages::view_page(&title, &origin, &src, &frames)
}

/// Renew the token while the page is open: the frame's origin, or the refusal that sends the page
/// to sign in or say why.
async fn renew(State(domain): State<ContentDomain>, request: Request) -> Response {
    let (parts, _) = request.into_parts();
    if !same_origin(&parts.headers, &domain.issuer) {
        return Problem::new(
            StatusCode::FORBIDDEN,
            "content-origin-refused",
            "This request's origin is refused",
        )
        .into_response();
    }
    let Some(canonical) = view_return_path(parts.uri.path()) else {
        return Problem::not_found().into_response();
    };
    let Some(opened) = domain.opened(&canonical) else {
        return Problem::not_found().into_response();
    };
    let session = match session_of(&domain, &parts.headers, &canonical).await {
        Ok(session) => session,
        Err(answer) => return *answer,
    };
    match domain.token(&opened, &session, &parts.headers, false).await {
        Ok(token) => Json(json!({"origin": domain.origin.under(&token)})).into_response(),
        Err(problem) => problem.into_response(),
    }
}
