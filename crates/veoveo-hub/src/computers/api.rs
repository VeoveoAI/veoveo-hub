//! The Computer's actions: `GET /computer/status`, `POST /computer/restart`, `POST
//! /computer/previous`, `POST /computer/keep-on` and `POST /computer/stop`, for the
//! owner's browser session, or for a bearer with a tenant whose token the Computers'
//! profile admits: the Computer's own Hub credential, or a server calling the Hub for
//! its caller. And `GET /computer/version`, the current version and its summary only,
//! public, for a server that names it in a tool's description.
use axum::{
    Extension, Json,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

use super::{
    ComputerRuntime,
    lifecycle::{Begun, Target},
    provisioning::Unavailable,
};
use crate::{AuthenticatedSubject, problem::Problem, store::Computer};

/// The caller's ready Computer, or the answer without one.
async fn owned(
    runtime: &ComputerRuntime,
    subject: &AuthenticatedSubject,
) -> Result<Computer, Box<Response>> {
    let principal = &subject.principal;
    let Some(tenant) = principal.tenant.as_ref() else {
        return Err(Box::new(
            Problem::new(
                StatusCode::FORBIDDEN,
                "computer-tenant-required",
                "A Computer belongs to a person in a tenant",
            )
            .into_response(),
        ));
    };
    match runtime
        .hub
        .hub_store()
        .owned_computer(tenant, &principal.id)
        .await
    {
        Ok(Some(computer)) if computer.ready => Ok(computer),
        Ok(_) => Err(Box::new(
            Problem::new(StatusCode::NOT_FOUND, "computer-not-found", "No Computer")
                .detail("the caller has no Computer set up; signing in at /computer sets one up")
                .into_response(),
        )),
        Err(error) => {
            tracing::error!("failed to read the caller's Computer: {error:#}");
            Err(Box::new(Problem::internal().into_response()))
        }
    }
}

fn unavailable(cause: &str) -> Response {
    Problem::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "computer-unavailable",
        "Computers are unavailable",
    )
    .detail(cause)
    .into_response()
}

/// No Computer can start now, for the cause the Hub logs when it changes: an expected
/// server error, not a failure of the request.
fn computers_unavailable(cause: &str) -> Response {
    crate::http::expected(unavailable(cause))
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// `GET /computer/status`: for a bearer, the caller's Computer's status as JSON, with
/// an update completed since the caller was last told in it once; for the owner's
/// browser session, the status page, with Restart. A browser without a session signs
/// in at `/computer`.
pub(super) async fn status(
    runtime: ComputerRuntime,
    subject: Option<Extension<AuthenticatedSubject>>,
    headers: HeaderMap,
) -> Response {
    let Some(Extension(subject)) = subject else {
        return browser_status(&runtime, &headers).await;
    };
    let computer = match owned(&runtime, &subject).await {
        Ok(computer) => computer,
        Err(response) => return *response,
    };
    match runtime.status(&computer, true).await {
        Ok(status) => no_store(Json(status).into_response()),
        Err(error) => {
            tracing::warn!(computer = %computer.id, "Computer status unavailable: {error:#}");
            unavailable("the Computer's state cannot be read now")
        }
    }
}

async fn browser_status(runtime: &ComputerRuntime, headers: &HeaderMap) -> Response {
    let result = async {
        let Some((_, session)) = super::oauth::admitted(runtime, headers).await? else {
            return super::oauth::redirect(super::pages::ENTRY);
        };
        let status = runtime.status(session.computer(), true).await?;
        Ok(super::pages::status(&status))
    }
    .await;
    result.unwrap_or_else(|error| {
        tracing::warn!(error = %error, "Computer status unavailable");
        super::oauth::refused(&error)
    })
}

/// `POST /computer/restart`: restart the caller's Computer at the current version.
pub(super) async fn restart(
    runtime: ComputerRuntime,
    subject: Option<Extension<AuthenticatedSubject>>,
    headers: HeaderMap,
) -> Response {
    act(runtime, subject, headers, Target::Current).await
}

/// `POST /computer/previous`: start the version recorded on the caller's Computer
/// again, after a failed restart.
pub(super) async fn previous(
    runtime: ComputerRuntime,
    subject: Option<Extension<AuthenticatedSubject>>,
    headers: HeaderMap,
) -> Response {
    act(runtime, subject, headers, Target::Previous).await
}

/// Begin a replacement of `target`. A bearer gets `202` with the status at once, or
/// `409` while one runs; the owner's browser session, from this origin only, goes to
/// `/computer`, which follows the replacement on the start page. The replacement
/// deletes the sandbox only after a short wait, so a caller inside the Computer receives
/// the answer first.
async fn act(
    runtime: ComputerRuntime,
    subject: Option<Extension<AuthenticatedSubject>>,
    headers: HeaderMap,
    target: Target,
) -> Response {
    let Some(Extension(subject)) = subject else {
        return browser_act(&runtime, &headers, target).await;
    };
    let computer = match owned(&runtime, &subject).await {
        Ok(computer) => computer,
        Err(response) => return *response,
    };
    match runtime.begin(&computer, target, true).await {
        Ok(Begun::Started) => {}
        Ok(Begun::Running) => {
            return Problem::new(
                StatusCode::CONFLICT,
                "computer-restart-running",
                "The Computer is already restarting",
            )
            .into_response();
        }
        Err(error) => {
            return match error.downcast_ref::<Unavailable>() {
                Some(cause) => computers_unavailable(&cause.0),
                None => {
                    tracing::warn!(computer = %computer.id, "Computer restart refused: {error:#}");
                    unavailable("the Computer cannot restart now")
                }
            };
        }
    }
    match runtime.status(&computer, false).await {
        Ok(status) => no_store((StatusCode::ACCEPTED, Json(status)).into_response()),
        Err(_) => no_store(StatusCode::ACCEPTED.into_response()),
    }
}

async fn browser_act(runtime: &ComputerRuntime, headers: &HeaderMap, target: Target) -> Response {
    if !super::proxy::same_origin(runtime, headers) {
        return super::oauth::private(
            Problem::new(
                StatusCode::FORBIDDEN,
                "computer-origin-refused",
                "Computer origin refused",
            )
            .into_response(),
        );
    }
    let result = async {
        let Some((_, session)) = super::oauth::admitted(runtime, headers).await? else {
            return Ok(super::oauth::denied());
        };
        // A replacement that already runs is followed on the start page too.
        runtime.begin(session.computer(), target, true).await?;
        super::oauth::redirect(super::pages::ENTRY)
    }
    .await;
    result.unwrap_or_else(|error| {
        tracing::warn!(error = %error, "Computer restart refused");
        super::oauth::refused(&error)
    })
}

/// The body of `POST /computer/keep-on`: `{"keepOn": true}` from a bearer, or
/// `keepOn=true` from the status page's form.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KeepOn {
    keep_on: bool,
}

fn keep_on_body(headers: &HeaderMap, body: &[u8]) -> Option<bool> {
    let json = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    if json {
        return serde_json::from_slice::<KeepOn>(body)
            .ok()
            .map(|body| body.keep_on);
    }
    match url::form_urlencoded::parse(body)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [(name, value)] if name == "keepOn" => match value.as_ref() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// `POST /computer/keep-on`: keep the caller's Computer on, so collection never stops
/// it, or no longer. It stays as set across restarts and updates. A bearer gets `204`;
/// the owner's browser session, from this origin only, goes to the status page.
pub(super) async fn keep_on(
    runtime: ComputerRuntime,
    subject: Option<Extension<AuthenticatedSubject>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let Some(keep_on) = keep_on_body(&headers, &body) else {
        return Problem::new(
            StatusCode::BAD_REQUEST,
            "computer-keep-on-invalid",
            "Invalid keep-on request",
        )
        .detail("the body names keepOn, true or false")
        .into_response();
    };
    let Some(Extension(subject)) = subject else {
        return browser_owned(&runtime, &headers, async |runtime, computer| {
            runtime
                .hub
                .hub_store()
                .keep_computer_on(computer, keep_on)
                .await?;
            super::oauth::redirect(super::pages::STATUS)
        })
        .await;
    };
    let computer = match owned_any(&runtime, &subject).await {
        Ok(computer) => computer,
        Err(response) => return *response,
    };
    match runtime
        .hub
        .hub_store()
        .keep_computer_on(&computer, keep_on)
        .await
    {
        Ok(()) => no_store(StatusCode::NO_CONTENT.into_response()),
        Err(error) => {
            tracing::error!(computer = %computer.id, "failed to keep the Computer on: {error:#}");
            Problem::internal().into_response()
        }
    }
}

/// `POST /computer/stop`: stop the caller's Computer now, as collection does whatever
/// its idle times or its keep-on: its sandbox is deleted and its home kept, and its next
/// entry starts it again. A bearer gets `204`, or `409` while an operation runs; the
/// owner's browser session, from this origin only, goes to the status page.
pub(super) async fn stop(
    runtime: ComputerRuntime,
    subject: Option<Extension<AuthenticatedSubject>>,
    headers: HeaderMap,
) -> Response {
    let Some(Extension(subject)) = subject else {
        return browser_owned(&runtime, &headers, async |runtime, computer| {
            super::collection::stop(&runtime.hub, &runtime.installation, computer).await?;
            super::oauth::redirect(super::pages::STATUS)
        })
        .await;
    };
    let computer = match owned_any(&runtime, &subject).await {
        Ok(computer) => computer,
        Err(response) => return *response,
    };
    match super::collection::stop(&runtime.hub, &runtime.installation, &computer).await {
        Ok(super::collection::Stop::Busy) => Problem::new(
            StatusCode::CONFLICT,
            "computer-operation-running",
            "The Computer is restarting or stopping",
        )
        .into_response(),
        Ok(_) => no_store(StatusCode::NO_CONTENT.into_response()),
        Err(error) => {
            tracing::warn!(computer = %computer.id, "Computer stop failed: {error:#}");
            unavailable("the Computer cannot stop now")
        }
    }
}

/// The caller's Computer, ready or not: one being started or stopped is still kept on,
/// or stopped.
async fn owned_any(
    runtime: &ComputerRuntime,
    subject: &AuthenticatedSubject,
) -> Result<Computer, Box<Response>> {
    let principal = &subject.principal;
    let Some(tenant) = principal.tenant.as_ref() else {
        return owned(runtime, subject).await;
    };
    match runtime
        .hub
        .hub_store()
        .owned_computer(tenant, &principal.id)
        .await
    {
        Ok(Some(computer)) => Ok(computer),
        _ => owned(runtime, subject).await,
    }
}

/// A browser action on the owner's Computer, from this origin only.
async fn browser_owned(
    runtime: &ComputerRuntime,
    headers: &HeaderMap,
    act: impl AsyncFnOnce(&ComputerRuntime, &Computer) -> anyhow::Result<Response>,
) -> Response {
    if !super::proxy::same_origin(runtime, headers) {
        return super::oauth::private(
            Problem::new(
                StatusCode::FORBIDDEN,
                "computer-origin-refused",
                "Computer origin refused",
            )
            .into_response(),
        );
    }
    let result = async {
        let Some((_, session)) = super::oauth::admitted(runtime, headers).await? else {
            return Ok(super::oauth::denied());
        };
        act(runtime, session.computer()).await
    }
    .await;
    result.unwrap_or_else(|error| {
        tracing::warn!(error = %error, "Computer action refused");
        super::oauth::refused(&error)
    })
}

/// The current version and its summary.
pub(super) async fn version(runtime: ComputerRuntime) -> Response {
    let current = match runtime.installation.current().await {
        Ok(current) => current,
        Err(cause) => return no_store(computers_unavailable(&cause.0)),
    };
    match current.spec.info() {
        Ok(info) => no_store(Json(info).into_response()),
        Err(error) => {
            tracing::warn!("the current Computer version has no digest: {error:#}");
            no_store(unavailable("the current Computer version cannot be named"))
        }
    }
}

#[cfg(test)]
mod tests;
