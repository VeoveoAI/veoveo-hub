//! The Computer's routes for bearers: `GET /computer/{profile}/status` and `POST
//! /computer/{profile}/restart` for the caller's own Computer, for any caller with a
//! tenant whose token the profile admits: the Computer's own Hub credential, or a
//! server calling the Hub for its caller. And `GET /computer/version`, the current
//! version and its summary, public, for a server that names it in a tool's description.
use axum::{
    Extension, Json,
    http::{HeaderValue, StatusCode, header},
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

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The caller's Computer's status. An update completed since the caller was last told
/// is in it once.
pub(super) async fn status(
    runtime: ComputerRuntime,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
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

/// Restart the caller's Computer at the current version. Answers 202 with its status
/// at once; the restart deletes the sandbox only after a short wait, so a caller inside
/// the Computer receives the answer first.
pub(super) async fn restart(
    runtime: ComputerRuntime,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let computer = match owned(&runtime, &subject).await {
        Ok(computer) => computer,
        Err(response) => return *response,
    };
    match runtime.begin(&computer, Target::Current, true).await {
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
                Some(cause) => unavailable(&cause.0),
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

/// The current version and its summary.
pub(super) async fn version(runtime: ComputerRuntime) -> Response {
    let current = match runtime.installation.current().await {
        Ok(current) => current,
        Err(cause) => return no_store(unavailable(&cause.0)),
    };
    match current.spec.info() {
        Ok(info) => no_store(Json(info).into_response()),
        Err(error) => {
            tracing::warn!("the current Computer version has no digest: {error:#}");
            no_store(unavailable("the current Computer version cannot be named"))
        }
    }
}
