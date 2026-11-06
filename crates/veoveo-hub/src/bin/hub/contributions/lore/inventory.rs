//! The factory as a person or an administrator may see it, in one read: `GET
//! /factory/{profile}/instances`. It answers from what the Hub holds and from Lore's
//! branches, and keeps nothing. See the design's factory inventory.
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

use anyhow::{Context, Result};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State, rejection::QueryRejection},
    http::{HeaderValue, StatusCode, header::CACHE_CONTROL},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, Utc};
use futures::{StreamExt, stream};
use serde::{Deserialize, Serialize};
use veoveo_hub::{
    AuthenticatedSubject, HubCatalog,
    contract::{HubAction, HubServerHealthState, PrincipalDisplayName},
    control_plane::PolicyTarget,
    execution::{BranchName, routes_for},
    factory::{Factory, InstanceStatus, InventoryEntry, Revision},
    http::ProfileAuthState,
    person::{Email, Person},
    problem::Problem,
};
use veoveo_hub_sdk::{ServerSlug, hub::LinkName};
use veoveo_types::{PrincipalId, TenantId};

use super::{
    GroupName, Lore, RepositoryId, Subject,
    factory::{
        Watched,
        store::RunnerFailure,
        watch::{BranchReport, BranchState, Watches},
    },
    sharing::Level,
};
use crate::{
    admin::admin_profile_id,
    audit::{AdminAuthorizationRequest, AdministrativeOperation, authorize_factory_read},
    runtime::{AdminState, current_catalog},
};

/// How many repositories' branches are read from Lore at once.
const LORE_READS: usize = 8;

#[derive(Clone)]
struct Inventory {
    admin: AdminState,
    factory: Factory,
    watches: Watches,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Scope {
    #[default]
    Own,
    Tenant,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    #[serde(default)]
    scope: Scope,
}

pub(crate) fn router(
    admin: AdminState,
    factory: Factory,
    watches: Watches,
    auth: ProfileAuthState,
) -> Router {
    Router::new()
        .route("/factory/{profile}/instances", get(inventory))
        .with_state(Inventory {
            admin,
            factory,
            watches,
        })
        .layer(axum::middleware::from_fn_with_state(
            auth,
            veoveo_hub::http::authenticate_profile,
        ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Answer {
    scope: Scope,
    /// Who asked, so that a view can say "you".
    caller: PrincipalId,
    read_at: DateTime<Utc>,
    base_branch: Option<BranchName>,
    routes: Vec<RouteView>,
    /// The servers the installation registered, not built from a branch.
    installed: Vec<ServerSlug>,
    repositories: Vec<RepositoryView>,
    databases: Vec<DatabaseView>,
    /// The people the answer names by principal who have signed in.
    people: Vec<PersonView>,
    /// Everyone's selections, for `scope=tenant`.
    #[serde(skip_serializing_if = "Option::is_none")]
    selections: Option<Vec<SelectionView>>,
}

/// Which branch serves the caller a server, and why it routes nothing when it does not.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RouteView {
    server: ServerSlug,
    /// None for a server the installation registered that no selection names.
    branch: Option<BranchName>,
    selected: bool,
    unavailable: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryView {
    id: RepositoryId,
    title: String,
    server: Option<ServerSlug>,
    /// Whether the caller's scope reaches the repository itself; a person who reaches only
    /// one of its links sees that link and the repository's title.
    reached: bool,
    /// Whether this repository is linked into another's branch.
    linked: bool,
    watch: Option<WatchView>,
    access: Vec<AccessView>,
    instances: Vec<InstanceView>,
    branches: Vec<BranchView>,
    /// Why Lore did not answer the repository's branches.
    branches_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WatchView {
    watcher: PrincipalId,
    failed: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AccessView {
    #[serde(skip_serializing_if = "Option::is_none")]
    principal: Option<PrincipalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<GroupName>,
    level: Level,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstanceView {
    branch: BranchName,
    link: Option<LinkName>,
    health: HubServerHealthState,
    /// None for a link whose repository the caller holds no grant on.
    detail: Option<InstanceDetail>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstanceDetail {
    server: Option<ServerSlug>,
    database: String,
    created_at: DateTime<Utc>,
    served_at: Option<DateTime<Utc>>,
    pusher: Option<PrincipalId>,
    /// Whether the latest pusher still holds a grant on the repository: the instance's
    /// Lore token is issued as them, and Lore refuses one without a grant.
    pusher_holds_grant: bool,
    linked_pusher: Option<PrincipalId>,
    status: InstanceStatus,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BranchView {
    name: String,
    latest: Revision,
    #[serde(flatten)]
    state: BranchState,
    link_failures: Vec<LinkFailureView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LinkFailureView {
    link: LinkName,
    #[serde(flatten)]
    failure: RunnerFailure,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DatabaseView {
    name: String,
    bytes: i64,
    /// The database that also keeps the Hub's own state: the base branch's.
    installation: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PersonView {
    principal: PrincipalId,
    name: PrincipalDisplayName,
    email: Option<Email>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectionView {
    principal: PrincipalId,
    server: ServerSlug,
    branch: BranchName,
}

async fn inventory(
    State(inventory): State<Inventory>,
    Path(profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    request: Result<Query<Request>, QueryRejection>,
) -> Response {
    let started_at = Instant::now();
    let scope = match request {
        Ok(Query(request)) => request.scope,
        Err(rejection) => {
            return Problem::new(
                StatusCode::BAD_REQUEST,
                "invalid-inventory",
                "Invalid inventory",
            )
            .detail(rejection.body_text())
            .into_response();
        }
    };
    let catalog = current_catalog(&inventory.admin.catalog);
    if scope == Scope::Tenant {
        let Some(profile_id) = admin_profile_id(profile) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let acting_server_installed = match &subject.delegation {
            None => false,
            Some(delegation) => match inventory
                .factory
                .is_instance(delegation.acting_server.deployment)
                .await
            {
                Ok(is_instance) => !is_instance,
                Err(error) => {
                    tracing::error!("the acting server is unknown: {error:#}");
                    return Problem::internal().into_response();
                }
            },
        };
        if let Err(response) = authorize_factory_read(
            &inventory.admin.hub_state,
            catalog.clone(),
            &profile_id,
            subject.clone(),
            AdminAuthorizationRequest {
                audit_target: None,
                action: HubAction::AdminRead.into(),
                target: PolicyTarget::Hub,
                operation: AdministrativeOperation::FactoryRead,
                started_at,
            },
            acting_server_installed,
        )
        .await
        {
            return *response;
        }
    }
    let Some(tenant) = subject.principal.tenant.clone() else {
        return Problem::new(StatusCode::FORBIDDEN, "inventory-forbidden", "No tenant")
            .detail("the caller belongs to no tenant")
            .into_response();
    };
    match read(&inventory, &catalog, &subject, &tenant, scope).await {
        Ok(answer) => (
            StatusCode::OK,
            [(CACHE_CONTROL, HeaderValue::from_static("no-store"))],
            Json(answer),
        )
            .into_response(),
        Err(error) => {
            tracing::error!("failed to read the factory's inventory: {error:#}");
            Problem::internal().into_response()
        }
    }
}

async fn read(
    inventory: &Inventory,
    catalog: &HubCatalog,
    subject: &AuthenticatedSubject,
    tenant: &TenantId,
    scope: Scope,
) -> Result<Answer> {
    let section = Lore::of(catalog)?;
    let deployment = section
        .sole_deployment()
        .map_err(|reason| anyhow::anyhow!("{reason}"))?;
    let caller = &subject.principal;
    let reaches =
        |repository: &RepositoryId| scope == Scope::Tenant || section.grants(caller, repository);

    let entries = inventory.factory.inventory().await?;
    let watched = inventory
        .watches
        .watches()
        .await?
        .into_iter()
        .filter(|(watched, _)| watched.deployment == deployment.id)
        .map(|(watched, watcher)| (watched.repository, watcher))
        .collect::<BTreeMap<_, _>>();

    let linked_repositories = entries
        .iter()
        .filter_map(linked_repository)
        .collect::<BTreeSet<_>>();

    let mut repositories = Vec::new();
    for repository in section
        .repositories
        .iter()
        .filter(|repository| &repository.tenant == tenant && repository.deployment == deployment.id)
    {
        let id = &repository.id;
        let own = entries
            .iter()
            .filter(|entry| entry.repository == id.as_str())
            .collect::<Vec<_>>();
        let reached = reaches(id);
        let instances = own
            .iter()
            .filter_map(|entry| {
                let holds = |pusher: &PrincipalId| {
                    section
                        .actions(&deployment.id, id, tenant, pusher)
                        .is_some()
                };
                instance_view(entry, reached, &reaches, holds)
            })
            .collect::<Vec<_>>();
        let linked = linked_repositories.contains(id);
        let relevant = reached && (watched.contains_key(id) || !own.is_empty() || linked);
        if !relevant && instances.is_empty() {
            continue;
        }
        let watch = if reached && watched.contains_key(id) {
            inventory
                .watches
                .watch_status(&Watched {
                    deployment: deployment.id.clone(),
                    repository: id.clone(),
                })
                .await?
                .map(|status| WatchView {
                    watcher: status.watcher,
                    failed: status.failed,
                })
        } else {
            None
        };
        let access = if reached {
            section
                .access(&deployment.id, id)
                .map(|assignment| AccessView {
                    principal: match &assignment.subject {
                        Subject::Principal(principal) => Some(principal.clone()),
                        Subject::Group(_) => None,
                    },
                    group: match &assignment.subject {
                        Subject::Principal(_) => None,
                        Subject::Group(group) => section
                            .groups
                            .iter()
                            .find(|candidate| &candidate.id == group)
                            .map(|group| group.name.clone()),
                    },
                    level: Level::of(&assignment.actions),
                })
                .collect()
        } else {
            Vec::new()
        };
        repositories.push(RepositoryView {
            id: id.clone(),
            title: repository.title.clone(),
            server: own.iter().find_map(|entry| entry.server.clone()),
            reached,
            linked,
            watch,
            access,
            instances,
            branches: Vec::new(),
            branches_error: None,
        });
    }

    // Lore's branches: as the caller for what their grants reach, as the watcher for the
    // whole factory. A linked repository's branches are its content, not servers.
    let reads = repositories
        .iter()
        .enumerate()
        .filter(|(_, repository)| repository.reached && !repository.linked)
        .filter_map(|(index, repository)| {
            let watch = repository.watch.as_ref()?;
            if watch.failed.is_some() {
                return None;
            }
            let person = match scope {
                Scope::Own => caller.id.clone(),
                Scope::Tenant => watch.watcher.clone(),
            };
            Some((index, repository.id.clone(), person))
        })
        .collect::<Vec<_>>();
    let results = stream::iter(reads)
        .map(|(index, id, person)| {
            let entries = &entries;
            let deployment = deployment.id.clone();
            async move {
                let watched = Watched {
                    deployment,
                    repository: id.clone(),
                };
                let served = |branch: &BranchName| {
                    entries
                        .iter()
                        .any(|entry| entry.repository == id.as_str() && &entry.branch == branch)
                };
                let read = inventory
                    .watches
                    .branch_states(&person, &watched, served)
                    .await;
                (index, read)
            }
        })
        .buffer_unordered(LORE_READS)
        .collect::<Vec<_>>()
        .await;
    for (index, read) in results {
        match read {
            Ok(reports) => {
                repositories[index].branches = reports.into_iter().map(branch_view).collect()
            }
            Err(error) => repositories[index].branches_error = Some(format!("{error:#}")),
        }
    }
    repositories.sort_by(|left, right| left.title.cmp(&right.title));

    let store = inventory.admin.hub_state.hub_store();
    let selections = match scope {
        Scope::Tenant => Some(store.tenant_selections(tenant).await?),
        Scope::Own => None,
    };
    let routes = routes_for(&inventory.admin.hub_state, catalog, &caller.id)
        .await
        .context("failed to read the caller's routes")?;
    let installed = catalog
        .registrations()
        .namespaces()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let built = entries
        .iter()
        .filter_map(|entry| entry.server.clone())
        .collect::<BTreeSet<_>>();
    let servers = built
        .iter()
        .chain(&installed)
        .chain(routes.selection.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let route_views = servers
        .into_iter()
        .map(|server| {
            let selected = routes.selection.get(&server).cloned();
            let branch = selected.clone().or_else(|| {
                built
                    .contains(&server)
                    .then(|| routes.base.clone())
                    .flatten()
            });
            RouteView {
                unavailable: routes.unavailable.get(&server).map(ToString::to_string),
                selected: selected.is_some(),
                branch,
                server,
            }
        })
        .collect();

    let database_names = repositories
        .iter()
        .flat_map(|repository| &repository.instances)
        .filter_map(|instance| instance.detail.as_ref())
        .map(|detail| detail.database.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let installation = inventory.factory.installation_database()?;
    let databases = inventory
        .factory
        .database_sizes(&database_names)
        .await?
        .into_iter()
        .map(|(name, bytes)| DatabaseView {
            installation: installation == name,
            name,
            bytes,
        })
        .collect();

    let mut named = BTreeSet::new();
    named.insert(caller.id.clone());
    for repository in &repositories {
        named.extend(repository.watch.iter().map(|watch| watch.watcher.clone()));
        named.extend(
            repository
                .access
                .iter()
                .filter_map(|access| access.principal.clone()),
        );
        for detail in repository
            .instances
            .iter()
            .filter_map(|i| i.detail.as_ref())
        {
            named.extend(detail.pusher.clone());
            named.extend(detail.linked_pusher.clone());
        }
    }
    named.extend(
        selections
            .iter()
            .flatten()
            .map(|(principal, _, _)| principal.clone()),
    );
    let people = store
        .people(tenant, &named.into_iter().collect::<Vec<_>>())
        .await?
        .into_iter()
        .map(
            |Person {
                 principal,
                 name,
                 email,
                 ..
             }| PersonView {
                principal,
                name,
                email,
            },
        )
        .collect();

    Ok(Answer {
        scope,
        caller: caller.id.clone(),
        read_at: Utc::now(),
        base_branch: routes.base,
        routes: route_views,
        installed,
        repositories,
        databases,
        people,
        selections: selections.map(|selections| {
            selections
                .into_iter()
                .map(|(principal, server, branch)| SelectionView {
                    principal,
                    server,
                    branch,
                })
                .collect()
        }),
    })
}

fn linked_repository(entry: &InventoryEntry) -> Option<RepositoryId> {
    entry
        .status
        .runner
        .as_ref()
        .and_then(|runner| RepositoryId::parse(runner.linked_repository.clone()).ok())
}

/// An instance as the caller's scope reaches it: whole, a link by name and health alone
/// for a person who reaches its parent and not its linked repository, or not at all.
fn instance_view(
    entry: &InventoryEntry,
    parent_reached: bool,
    reaches: &impl Fn(&RepositoryId) -> bool,
    holds_grant: impl Fn(&PrincipalId) -> bool,
) -> Option<InstanceView> {
    let link = entry
        .status
        .runner
        .as_ref()
        .map(|runner| runner.link.clone());
    let revealed = match linked_repository(entry) {
        Some(linked) => reaches(&linked),
        None => parent_reached,
    };
    if !revealed && !parent_reached {
        return None;
    }
    Some(InstanceView {
        branch: entry.branch.clone(),
        link,
        health: entry.status.health().state,
        detail: revealed.then(|| InstanceDetail {
            server: entry.server.clone(),
            database: entry.database.clone(),
            created_at: entry.created_at,
            served_at: entry.served_at,
            pusher_holds_grant: entry.pusher.as_ref().is_some_and(&holds_grant),
            pusher: entry.pusher.clone(),
            linked_pusher: entry.linked_pusher.clone(),
            status: entry.status.clone(),
        }),
    })
}

fn branch_view(report: BranchReport) -> BranchView {
    BranchView {
        name: report.name,
        latest: report.latest,
        state: report.state,
        link_failures: report
            .link_failures
            .into_iter()
            .map(|(link, failure)| LinkFailureView { link, failure })
            .collect(),
    }
}
