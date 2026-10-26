//! Owners share repositories with people and groups. `GET` and `PUT
//! /lore/{profile}/repositories/{id}/access` read and set who may reach a repository of
//! the installation's one Lore deployment; `GET` and `POST /lore/{profile}/groups` list the
//! caller's groups and create one; `PUT /lore/{profile}/groups/{name}/members` sets a
//! membership. They take any authenticated caller with a tenant, directly or through a
//! server that calls the Hub for them, as the creation route does; what a caller may do is
//! decided by the section's assignments and memberships (`changes.rs`). People are named
//! by the email they signed in with (`hub.person`) and groups by name; IDs stay inside the
//! Hub. Each change publishes one control-plane revision and answers once it is active in
//! this process.
mod changes;

use std::collections::BTreeSet;

use anyhow::Result;
use axum::{
    Extension, Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header::CACHE_CONTROL},
    response::{IntoResponse, Response},
    routing::{get, put},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use veoveo_hub::{
    AuthenticatedSubject,
    contract::PrincipalDisplayName,
    control_plane::HubControlPlaneRevisionSource,
    http::ProfileAuthState,
    new_hub_control_plane_revision_id,
    person::{Email, Person},
    problem::Problem,
};
use veoveo_types::{PrincipalId, TenantId};

pub(crate) use changes::{Caller, Level, Named, Refusal, Setting};

use super::{
    Deployment, Group, GroupId, GroupName, GroupRole, Lore, NativeAction, RepositoryId, Subject,
    publication::{Author, Publisher},
    repositories::random_hex,
};
use crate::runtime::AdminState;

pub(crate) fn router(state: AdminState, auth: ProfileAuthState) -> Router {
    Router::new()
        .route(
            "/lore/{profile}/repositories/{id}/access",
            get(read_access).put(set_access),
        )
        .route(
            "/lore/{profile}/groups",
            get(read_groups).post(create_group),
        )
        .route("/lore/{profile}/groups/{name}/members", put(set_membership))
        .with_state(state)
        .layer(axum::middleware::from_fn_with_state(
            auth,
            veoveo_hub::http::authenticate_profile,
        ))
}

/// The body of `PUT …/access`: exactly one of `person`, an email, and `group`, a name.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessChange {
    #[serde(default)]
    person: Option<String>,
    #[serde(default)]
    group: Option<String>,
    access: Setting,
}

/// The body of `POST …/groups`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewGroup {
    name: String,
}

/// The body of `PUT …/groups/{name}/members`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MembershipChange {
    person: String,
    role: Setting,
}

/// A person as the answers name them: by the email and name of their last sign-in, both
/// null for one who never signed in, such as one an administrator assigned.
#[derive(Serialize)]
struct PersonView {
    email: Option<Email>,
    name: Option<PrincipalDisplayName>,
}

#[derive(Serialize)]
struct GroupRef {
    name: GroupName,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum SubjectView {
    Person(PersonView),
    Group(GroupRef),
}

#[derive(Serialize)]
struct AccessEntry {
    #[serde(flatten)]
    subject: SubjectView,
    access: Level,
    actions: BTreeSet<NativeAction>,
}

#[derive(Serialize)]
struct AccessList {
    repository: RepositoryId,
    access: Vec<AccessEntry>,
}

#[derive(Serialize)]
struct MemberView {
    email: Option<Email>,
    name: Option<PrincipalDisplayName>,
    role: GroupRole,
}

#[derive(Serialize)]
struct GroupView {
    name: GroupName,
    members: Vec<MemberView>,
}

#[derive(Serialize)]
struct GroupList {
    groups: Vec<GroupView>,
}

async fn read_access(
    State(state): State<AdminState>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Path((_, id)): Path<(String, String)>,
) -> Result<Response, Problem> {
    let (tenant, deployment, section) = context(&state, &subject)?;
    let id = RepositoryId::parse(&id).map_err(|_| refused(Refusal::UnknownRepository))?;
    let caller = Caller {
        tenant: &tenant,
        principal: &subject.principal.id,
    };
    section
        .readable(&deployment.id, &id, caller)
        .map_err(refused)?;
    access_list(&state, &tenant, &deployment, &section, id).await
}

async fn set_access(
    State(state): State<AdminState>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Path((_, id)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response, Problem> {
    let (tenant, deployment, _) = context(&state, &subject)?;
    let id = RepositoryId::parse(&id).map_err(|_| refused(Refusal::UnknownRepository))?;
    let change = parse::<AccessChange>(
        &body,
        r#"The body is {"person": "<email>", "access": "member", "owner" or "none"} or {"group": "<name>", "access": "member", "owner" or "none"}."#,
    )?;
    let named = match (&change.person, &change.group) {
        (Some(email), None) => Named::Person(person(&state, &tenant, email).await?.principal),
        (None, Some(name)) => {
            Named::Group(GroupName::parse(name).map_err(|_| refused(Refusal::UnknownGroup))?)
        }
        _ => {
            return Err(invalid_request(
                "Name exactly one of person, an email, and group, a group name.",
            ));
        }
    };
    let caller = Caller {
        tenant: &tenant,
        principal: &subject.principal.id,
    };
    let section = publish(&state, &subject, &tenant, |section| {
        Ok(section.set_access(&deployment.id, &id, caller, &named, change.access)?)
    })
    .await?;
    access_list(&state, &tenant, &deployment, &section, id).await
}

async fn read_groups(
    State(state): State<AdminState>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Result<Response, Problem> {
    let (tenant, _, section) = context(&state, &subject)?;
    let mut groups = section
        .memberships(&tenant, &subject.principal.id)
        .collect::<Vec<_>>();
    groups.sort_by(|a, b| a.name.cmp(&b.name));
    let principals = groups
        .iter()
        .flat_map(|group| group.members.iter().map(|m| m.principal.clone()))
        .collect();
    let people = people(&state, &tenant, principals).await?;
    let groups = groups
        .into_iter()
        .map(|group| group_view(group, &people))
        .collect();
    Ok(answer(StatusCode::OK, &GroupList { groups }))
}

async fn create_group(
    State(state): State<AdminState>,
    Extension(subject): Extension<AuthenticatedSubject>,
    body: Bytes,
) -> Result<Response, Problem> {
    let (tenant, _, _) = context(&state, &subject)?;
    let request = parse::<NewGroup>(&body, r#"The body is {"name": "<group name>"}."#)?;
    let name = GroupName::parse(&request.name).map_err(|_| {
        Problem::new(
            StatusCode::BAD_REQUEST,
            "invalid-group-name",
            "Invalid group name",
        )
        .detail("A group name is 1 to 64 of a-z, 0-9 and -, starting with a letter.")
    })?;
    let caller = Caller {
        tenant: &tenant,
        principal: &subject.principal.id,
    };
    let section = publish(&state, &subject, &tenant, |section| {
        section.create_group(caller, &name, GroupId::parse(random_hex()?)?)?;
        Ok(true)
    })
    .await?;
    group_answer(&state, &tenant, &section, &name, StatusCode::CREATED).await
}

async fn set_membership(
    State(state): State<AdminState>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Path((_, name)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response, Problem> {
    let (tenant, _, _) = context(&state, &subject)?;
    let name = GroupName::parse(&name).map_err(|_| refused(Refusal::UnknownGroup))?;
    let change = parse::<MembershipChange>(
        &body,
        r#"The body is {"person": "<email>", "role": "member", "owner" or "none"}."#,
    )?;
    let member = person(&state, &tenant, &change.person).await?.principal;
    let caller = Caller {
        tenant: &tenant,
        principal: &subject.principal.id,
    };
    let section = publish(&state, &subject, &tenant, |section| {
        Ok(section.set_membership(caller, &name, &member, change.role)?)
    })
    .await?;
    group_answer(&state, &tenant, &section, &name, StatusCode::OK).await
}

/// The caller's tenant, the installation's one Lore deployment and the active Lore
/// section.
fn context(
    state: &AdminState,
    subject: &AuthenticatedSubject,
) -> Result<(TenantId, Deployment, Lore), Problem> {
    let tenant = subject.principal.tenant.clone().ok_or_else(|| {
        Problem::new(
            StatusCode::FORBIDDEN,
            "sharing-forbidden",
            "Sharing requires a tenant",
        )
        .detail("You belong to no tenant, so you have no repositories or groups to share.")
    })?;
    let section = Lore::of(&state.catalog.current()).map_err(|error| {
        tracing::error!("the Lore section is unreadable: {error:#}");
        unavailable()
    })?;
    let deployment = section.sole_deployment().cloned().map_err(|reason| {
        tracing::error!("sharing has no Lore deployment: {reason}");
        unavailable()
    })?;
    Ok((tenant, deployment, section))
}

/// Publish what `change` makes of the active Lore section, as the caller, and answer the
/// section it left: the published one, or the active one when nothing changed.
async fn publish(
    state: &AdminState,
    subject: &AuthenticatedSubject,
    tenant: &TenantId,
    mut change: impl FnMut(&mut Lore) -> Result<bool>,
) -> Result<Lore, Problem> {
    let audit = subject.audit_context();
    let author = Author {
        source: HubControlPlaneRevisionSource::Contribution,
        principal: &subject.principal.id,
        tenant: Some(tenant),
        audit: &audit,
    };
    let mut result = None;
    let published = Publisher::of(state)
        .publish(
            &author,
            new_hub_control_plane_revision_id,
            || async { Ok(true) },
            |_, section| {
                let changed = change(section)?;
                result = Some(section.clone());
                Ok(changed)
            },
        )
        .await;
    match published {
        Ok(_) => Ok(result.expect("a publication runs its change")),
        Err(error) => match error.downcast::<Refusal>() {
            Ok(refusal) => Err(refused(refusal)),
            Err(error) => {
                tracing::error!("a sharing change was not published: {error:#}");
                Err(unavailable())
            }
        },
    }
}

/// The person of the tenant who signed in with `email`.
async fn person(state: &AdminState, tenant: &TenantId, email: &str) -> Result<Person, Problem> {
    let email = Email::parse(email)
        .map_err(|_| invalid_request("A person is named by an email address."))?;
    match state
        .hub_state
        .hub_store()
        .person_by_email(tenant, &email)
        .await
    {
        Ok(Some(person)) => Ok(person),
        Ok(None) => Err(Problem::new(
            StatusCode::NOT_FOUND,
            "person-not-found",
            "No one has signed in with this email",
        )
        .detail(
            "People are named by the email they sign in to the Hub with, and no one in your \
             tenant has signed in with this one. Ask them to sign in once, then try again.",
        )),
        Err(error) => {
            tracing::error!("failed to read a person by email: {error:#}");
            Err(unavailable())
        }
    }
}

/// The email and name of each of `principals` who signed in.
async fn people(
    state: &AdminState,
    tenant: &TenantId,
    principals: BTreeSet<PrincipalId>,
) -> Result<Vec<Person>, Problem> {
    let principals = principals.into_iter().collect::<Vec<_>>();
    state
        .hub_state
        .hub_store()
        .people(tenant, &principals)
        .await
        .map_err(|error| {
            tracing::error!("failed to read people: {error:#}");
            unavailable()
        })
}

fn person_view(principal: &PrincipalId, people: &[Person]) -> PersonView {
    let known = people.iter().find(|person| &person.principal == principal);
    PersonView {
        email: known.and_then(|person| person.email.clone()),
        name: known.map(|person| person.name.clone()),
    }
}

/// The repository's access: people by email, those without one last, then groups by
/// name.
async fn access_list(
    state: &AdminState,
    tenant: &TenantId,
    deployment: &Deployment,
    section: &Lore,
    repository: RepositoryId,
) -> Result<Response, Problem> {
    let assignments = section
        .access(&deployment.id, &repository)
        .collect::<Vec<_>>();
    let principals = assignments
        .iter()
        .filter_map(|a| match &a.subject {
            Subject::Principal(principal) => Some(principal.clone()),
            Subject::Group(_) => None,
        })
        .collect();
    let people = people(state, tenant, principals).await?;
    let mut persons = Vec::new();
    let mut groups = Vec::new();
    for assignment in assignments {
        let entry = |subject| AccessEntry {
            subject,
            access: Level::of(&assignment.actions),
            actions: assignment.actions.clone(),
        };
        match &assignment.subject {
            Subject::Principal(principal) => {
                let view = person_view(principal, &people);
                persons.push((principal, entry(SubjectView::Person(view))));
            }
            Subject::Group(id) => {
                // Admission keeps every group an assignment names.
                if let Some(group) = section.groups.iter().find(|g| &g.id == id) {
                    groups.push(entry(SubjectView::Group(GroupRef {
                        name: group.name.clone(),
                    })));
                }
            }
        }
    }
    persons.sort_by(|(a, x), (b, y)| order(x).cmp(&order(y)).then(a.cmp(b)));
    groups.sort_by(|x, y| order(x).cmp(&order(y)));
    let access = persons
        .into_iter()
        .map(|(_, entry)| entry)
        .chain(groups)
        .collect();
    Ok(answer(StatusCode::OK, &AccessList { repository, access }))
}

/// An entry's place in a list: by email, those without one last, or by group name.
fn order(entry: &AccessEntry) -> (bool, Option<&str>) {
    match &entry.subject {
        SubjectView::Person(person) => (
            person.email.is_none(),
            person.email.as_ref().map(Email::as_str),
        ),
        SubjectView::Group(group) => (false, Some(group.name.as_str())),
    }
}

/// A group with its members by email, those without one last.
fn group_view(group: &Group, people: &[Person]) -> GroupView {
    let mut members = group
        .members
        .iter()
        .map(|member| {
            let person = person_view(&member.principal, people);
            (
                &member.principal,
                MemberView {
                    email: person.email,
                    name: person.name,
                    role: member.role,
                },
            )
        })
        .collect::<Vec<_>>();
    members.sort_by(|(a, x), (b, y)| {
        (x.email.is_none(), &x.email)
            .cmp(&(y.email.is_none(), &y.email))
            .then(a.cmp(b))
    });
    GroupView {
        name: group.name.clone(),
        members: members.into_iter().map(|(_, member)| member).collect(),
    }
}

async fn group_answer(
    state: &AdminState,
    tenant: &TenantId,
    section: &Lore,
    name: &GroupName,
    status: StatusCode,
) -> Result<Response, Problem> {
    let group = section
        .group_named(tenant, name)
        .ok_or_else(|| refused(Refusal::UnknownGroup))?;
    let principals = group.members.iter().map(|m| m.principal.clone()).collect();
    let people = people(state, tenant, principals).await?;
    Ok(answer(status, &group_view(group, &people)))
}

fn answer(status: StatusCode, body: &impl Serialize) -> Response {
    let mut response = (status, Json(body)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The JSON body, or a 400 that says what it is.
fn parse<T: DeserializeOwned>(body: &[u8], shape: &'static str) -> Result<T, Problem> {
    serde_json::from_slice(body).map_err(|_| invalid_request(shape))
}

fn invalid_request(detail: &'static str) -> Problem {
    Problem::new(
        StatusCode::BAD_REQUEST,
        "invalid-request",
        "Invalid request",
    )
    .detail(detail)
}

fn unavailable() -> Problem {
    Problem::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "sharing-unavailable",
        "Sharing is unavailable",
    )
    .detail("The Hub could not read or publish who may reach Lore repositories. Try again later.")
}

/// A refusal as the caller's problem.
fn refused(refusal: Refusal) -> Problem {
    let (status, code, title, detail) = match refusal {
        Refusal::UnknownRepository => (
            StatusCode::NOT_FOUND,
            "repository-not-found",
            "Unknown repository",
            "No Lore repository of this ID is registered in your tenant.",
        ),
        Refusal::NotGranted => (
            StatusCode::FORBIDDEN,
            "repository-access-forbidden",
            "No access to the repository",
            "You hold no access to this repository. Ask one of its owners to share it with you.",
        ),
        Refusal::NotRepositoryOwner => (
            StatusCode::FORBIDDEN,
            "repository-owner-required",
            "Only an owner shares the repository",
            "You are not an owner of this repository. Ask one of its owners to change who may \
             reach it.",
        ),
        Refusal::UnknownGroup => (
            StatusCode::NOT_FOUND,
            "group-not-found",
            "Unknown group",
            "Your tenant has no group of this name. Check the name, or create the group first.",
        ),
        Refusal::NotGroupOwner => (
            StatusCode::FORBIDDEN,
            "group-owner-required",
            "Only an owner changes the group",
            "You are not an owner of this group. Ask one of its owners to change its members.",
        ),
        Refusal::GroupTaken => (
            StatusCode::CONFLICT,
            "group-exists",
            "Group name taken",
            "Your tenant has a group of this name already. Choose another name.",
        ),
        Refusal::KeepRepositoryOwner => (
            StatusCode::CONFLICT,
            "repository-owner-kept",
            "You would no longer own the repository",
            "This change would leave you without owner access to the repository, so it was not \
             made. To give up ownership, make sure someone else owns it and ask them to change \
             your access.",
        ),
        Refusal::KeepGroupOwner => (
            StatusCode::CONFLICT,
            "group-owner-kept",
            "You would no longer own the group",
            "This change would leave you without the owner role in the group, so it was not \
             made. To give up ownership, make another member an owner and ask them to change \
             your role.",
        ),
    };
    Problem::new(status, code, title).detail(detail)
}
