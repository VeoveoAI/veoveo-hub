//! A person's Lore token, as the Hub issues it when it acts in Lore for them: the token
//! the deployment's public client would obtain for the person through the Hub's
//! exchange, admitted against their current assignments on every issue.
use anyhow::{Context, Result};
use veoveo_hub::{
    HubCatalog,
    contract::{OAuthClientAuthMethod, OAuthGrantType},
    control_plane::OAuthClientRegistration,
    issuance::IssuedAccessToken,
    token_profile::{
        ProfileRequest, TokenContext, TokenIssueError, authorize_token, issue_profile_token,
    },
};
use veoveo_hub_sdk::{Principal, PrincipalKind, TokenIssuer, TokenSubject};
use veoveo_types::{PrincipalId, TenantId};

use super::{Deployment, DeploymentId, Lore, RepositoryId, Request};

/// Issue `person` a token for the deployment's repository under their current grant:
/// the token the person would obtain through the Hub's exchange with the deployment's
/// public client. The Lore profile admits the person's assignment on every issue.
pub(crate) async fn issue(
    catalog: &HubCatalog,
    person: &PrincipalId,
    deployment: &DeploymentId,
    repository: &RepositoryId,
) -> Result<IssuedAccessToken, TokenIssueError> {
    let section = Lore::of(catalog).map_err(TokenIssueError::Unavailable)?;
    let deployment = section
        .deployments
        .iter()
        .find(|candidate| &candidate.id == deployment)
        .with_context(|| format!("the Lore deployment {deployment} is not registered"))
        .map_err(TokenIssueError::Unavailable)?;
    let tenant = section
        .repositories
        .iter()
        .find(|registered| registered.deployment == deployment.id && &registered.id == repository)
        .map(|registered| registered.tenant.clone())
        .with_context(|| format!("the Lore repository {repository} is not registered"))?;
    let principal = person_of(catalog, person, tenant)?;
    let client = exchange_client(catalog, deployment).map_err(TokenIssueError::Unavailable)?;
    let context = admit(catalog, deployment, client, &principal)?;
    let scope = deployment.scope_template.render(repository);
    crate::contributions::bindings()
        .map_err(TokenIssueError::Unavailable)?
        .profiles
        .issue(
            context,
            ProfileRequest::Exchange {
                resource: None,
                scope: Some(&scope),
            },
            None,
        )
        .await
}

/// Issue `person` of `tenant` one token for every repository they are assigned on the
/// deployment, with one resource entry each and none without an assignment, and no
/// scope: the deployment's public client would obtain the same entries one repository
/// at a time.
pub(crate) async fn issue_assigned(
    catalog: &HubCatalog,
    deployment: &DeploymentId,
    tenant: &TenantId,
    person: &PrincipalId,
) -> Result<IssuedAccessToken, TokenIssueError> {
    issue_typed(catalog, deployment, tenant, person, &Request::Assigned).await
}

/// Issue `person` of `tenant` a Lore login token for the deployment: it names the person
/// and carries no repository rights, which is what Lore requires to create a repository.
pub(crate) async fn issue_login(
    catalog: &HubCatalog,
    deployment: &DeploymentId,
    tenant: &TenantId,
    person: &PrincipalId,
) -> Result<IssuedAccessToken, TokenIssueError> {
    issue_typed(catalog, deployment, tenant, person, &Request::Login).await
}

/// Issue `person` of `tenant` the token of a typed request of the Lore profile.
async fn issue_typed(
    catalog: &HubCatalog,
    deployment: &DeploymentId,
    tenant: &TenantId,
    person: &PrincipalId,
    request: &Request,
) -> Result<IssuedAccessToken, TokenIssueError> {
    let section = Lore::of(catalog).map_err(TokenIssueError::Unavailable)?;
    let deployment = section
        .deployments
        .iter()
        .find(|candidate| &candidate.id == deployment)
        .with_context(|| format!("the Lore deployment {deployment} is not registered"))
        .map_err(TokenIssueError::Unavailable)?;
    let principal = person_of(catalog, person, tenant.clone())?;
    let client = exchange_client(catalog, deployment).map_err(TokenIssueError::Unavailable)?;
    let context = admit(catalog, deployment, client, &principal)?;
    let bindings = crate::contributions::bindings().map_err(TokenIssueError::Unavailable)?;
    let token = authorize_token(
        &bindings.lore,
        context,
        request,
        &bindings.profiles.claim_registry(),
    )?;
    issue_profile_token(token, None).await
}

/// The person's token context with the deployment's public client and login scopes.
fn admit<'a>(
    catalog: &'a HubCatalog,
    deployment: &'a Deployment,
    client: &'a OAuthClientRegistration,
    principal: &'a Principal,
) -> Result<TokenContext<'a>, TokenIssueError> {
    TokenContext::admit(
        catalog,
        &deployment.audience,
        &client.id,
        principal,
        &client.default_work_context,
        &deployment.login_scopes,
        None,
    )
    .map_err(TokenIssueError::Unavailable)
}

/// The Hub principal of `id` in the repository's tenant. A Hub principal ID names its
/// issuer and subject, `<issuer>#<subject>`; one issued by the Hub's own authorization
/// servers is a service.
fn person_of(catalog: &HubCatalog, id: &PrincipalId, tenant: TenantId) -> Result<Principal> {
    let (issuer, subject) = id
        .as_str()
        .split_once('#')
        .with_context(|| format!("the principal {id} names no issuer and subject"))?;
    let issuer = TokenIssuer::parse(issuer)?;
    let kind = if catalog
        .control_plane()
        .authorization_servers
        .iter()
        .any(|server| server.issuer == issuer)
    {
        PrincipalKind::Service
    } else {
        PrincipalKind::User
    };
    Ok(Principal {
        id: id.clone(),
        kind,
        issuer,
        subject: TokenSubject::parse(subject)?,
        tenant: Some(tenant),
        groups: Default::default(),
        group_roles: Default::default(),
        roles: Default::default(),
        scopes: Default::default(),
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: None,
    })
}

/// The deployment's one public client that exchanges login tokens for repository tokens.
fn exchange_client<'a>(
    catalog: &'a HubCatalog,
    deployment: &Deployment,
) -> Result<&'a OAuthClientRegistration> {
    let mut clients = catalog
        .control_plane()
        .oauth_clients
        .iter()
        .filter(|client| {
            client.authorization_server == deployment.authorization_server
                && client.allowed_resources.contains(&deployment.audience)
                && client.grant_types.contains(&OAuthGrantType::TokenExchange)
                && client.auth_methods
                    == std::collections::BTreeSet::from([OAuthClientAuthMethod::None])
        });
    let client = clients.next().with_context(|| {
        format!(
            "no public client exchanges tokens for the Lore deployment {}",
            deployment.id
        )
    })?;
    anyhow::ensure!(
        clients.next().is_none(),
        "more than one public client exchanges tokens for the Lore deployment {}",
        deployment.id
    );
    Ok(client)
}
