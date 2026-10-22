//! Per-user selection: the branch whose instance serves each server to one principal.
//! A principal sets and clears only its own selection, directly or through a server
//! that calls the Hub for it, so the route needs no authority beyond authentication.
//! Every replica learns of a change on the `hub` channel and completes that
//! principal's open MCP listens with a catalog change hint, so its clients list again
//! under the new selection.

use axum::{
    Extension, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::put,
};
use futures::StreamExt;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use veoveo_hub_sdk::ServerSlug;
use veoveo_store::ChangeSignal;
use veoveo_types::PrincipalId;

use crate::{
    AuthenticatedSubject, HubState, execution::BranchName, http::ProfileAuthState, problem::Problem,
};

/// The table whose changes are selection changes.
const SELECTION_TABLE: &str = "selection";

/// The body of `PUT /selection/{profile}/{server}`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    branch: BranchName,
}

/// `PUT /selection/{profile}/{server}` with `{"branch": "…"}` serves `server` to the
/// caller from the instance of that branch; `DELETE` of the same path returns it to
/// the base branch. Both answer 204. The profile only authenticates the caller.
pub fn router(hub: HubState, auth: ProfileAuthState) -> Router {
    Router::new()
        .route("/selection/{profile}/{server}", put(select).delete(clear))
        .with_state(hub)
        .layer(axum::middleware::from_fn_with_state(
            auth,
            crate::http::authenticate_profile,
        ))
}

async fn select(
    State(hub): State<HubState>,
    Path((_, server)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    veoveo_http::RequestJson(selection): veoveo_http::RequestJson<Selection>,
) -> Response {
    let Ok(server) = ServerSlug::parse(server) else {
        return invalid_server();
    };
    let selected = hub
        .hub_store()
        .select(&subject.principal.id, &server, &selection.branch)
        .await;
    answer(selected)
}

async fn clear(
    State(hub): State<HubState>,
    Path((_, server)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let Ok(server) = ServerSlug::parse(server) else {
        return invalid_server();
    };
    let cleared = hub
        .hub_store()
        .clear_selection(&subject.principal.id, &server)
        .await;
    answer(cleared)
}

fn invalid_server() -> Response {
    Problem::new(StatusCode::BAD_REQUEST, "invalid-server", "Invalid server")
        .detail("the path does not name a server slug")
        .into_response()
}

fn answer(result: anyhow::Result<()>) -> Response {
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => {
            tracing::error!("selection change failed: {error:#}");
            Problem::internal().into_response()
        }
    }
}

/// Tell this replica's listens which principals' selections changed, until `stop`
/// fires. A change missed while the listener reconnects only delays a client until
/// its catalog's lifetime ends; routing always reads the current selection.
pub fn spawn_watcher(hub: HubState, stop: CancellationToken) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut changes = hub.platform_store().listen(vec![crate::store::channel()]);
        loop {
            let signal = tokio::select! {
                () = stop.cancelled() => return,
                signal = changes.next() => signal,
            };
            match signal {
                Some(Ok(ChangeSignal::Changed(change))) if change.table == SELECTION_TABLE => {
                    if let Some(principal) = change.key.and_then(|key| PrincipalId::parse(key).ok())
                    {
                        // No receiver means no open listen.
                        let _ = hub.selection_changes.send(principal);
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(error)) => tracing::warn!("selection notifications interrupted: {error}"),
                None => return,
            }
        }
    })
}
