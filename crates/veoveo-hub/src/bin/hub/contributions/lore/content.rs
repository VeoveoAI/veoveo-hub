//! A Lore repository's branch as an item on the Hub's content domain:
//! `/view/lore/{repository}/{branch}/{path}`, read as the person with [`files::read`], so Lore's
//! rules decide each read and a push is live on the next request.
use anyhow::Result;
use axum::{
    http::{HeaderMap, Method},
    response::Response,
};
use futures::future::BoxFuture;
use percent_encoding::percent_decode_str;
use veoveo_hub::{
    HubCatalog,
    computers::BrowserSession,
    content::{ContentOwner, Named},
    problem::Problem,
};
use veoveo_hub_sdk::Principal;

use super::{
    Lore, RepositoryId,
    files::{self, FilePath, FileRead},
    sharing::{Caller, Refusal, refused},
};

/// The owner name of Lore's items.
pub(crate) const OWNER: &str = "lore";

pub(crate) struct LoreContent {
    http: reqwest::Client,
}

impl LoreContent {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            http: files::client()?,
        })
    }
}

/// An item's repository and its branch's name.
fn item_parts(item: &str) -> Option<(RepositoryId, String)> {
    let (repository, branch) = item.split_once('/')?;
    let branch = percent_decode_str(branch).decode_utf8().ok()?.into_owned();
    Some((RepositoryId::parse(repository).ok()?, branch))
}

/// The repository's section and deployment, after `person`'s grant on it has been checked.
fn admitted(
    catalog: &HubCatalog,
    person: &Principal,
    repository: &RepositoryId,
) -> Result<super::Deployment, Problem> {
    let tenant = person
        .tenant
        .as_ref()
        .ok_or_else(|| refused(Refusal::NotGranted))?;
    let section = Lore::of(catalog).map_err(|error| {
        tracing::error!("the Lore section is unreadable: {error:#}");
        files::unavailable()
    })?;
    let deployment = section.sole_deployment().map_err(|reason| {
        tracing::error!("Lore content has no deployment: {reason}");
        files::unavailable()
    })?;
    section
        .readable(
            &deployment.id,
            repository,
            Caller {
                tenant,
                principal: &person.id,
            },
        )
        .map_err(refused)?;
    Ok(deployment.clone())
}

impl ContentOwner for LoreContent {
    fn named(&self, rest: &str) -> Option<Named> {
        let (repository, rest) = rest.split_once('/')?;
        RepositoryId::parse(repository).ok()?;
        let (branch, path) = rest.split_once('/').unwrap_or((rest, ""));
        if percent_decode_str(branch).decode_utf8().ok()?.is_empty() {
            return None;
        }
        FilePath::decode(path)?;
        Some(Named {
            item: format!("{repository}/{branch}"),
            path: path.to_owned(),
        })
    }

    fn admit<'a>(
        &'a self,
        catalog: &'a HubCatalog,
        session: &'a BrowserSession,
        item: &'a str,
    ) -> BoxFuture<'a, Result<(), Problem>> {
        Box::pin(async move {
            let (repository, _) = item_parts(item).ok_or_else(Problem::not_found)?;
            admitted(catalog, session.principal(), &repository).map(|_| ())
        })
    }

    fn read<'a>(
        &'a self,
        catalog: &'a HubCatalog,
        session: &'a BrowserSession,
        item: &'a str,
        path: &'a str,
        _method: &'a Method,
        headers: &'a HeaderMap,
    ) -> BoxFuture<'a, Result<Response, Problem>> {
        let person = session.principal();
        Box::pin(async move {
            let (repository, branch) = item_parts(item).ok_or_else(Problem::not_found)?;
            let path = FilePath::decode(path).ok_or_else(Problem::not_found)?;
            let deployment = admitted(catalog, person, &repository)?;
            files::read(
                catalog,
                &self.http,
                &person.id,
                FileRead {
                    deployment: &deployment,
                    repository: &repository,
                    branch: &branch,
                    path: &path,
                },
                headers,
            )
            .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPOSITORY: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn a_url_names_a_repositorys_branch_and_a_path_in_it() {
        let content = LoreContent::new().unwrap();
        let named = |rest: &str| content.named(rest);
        assert_eq!(
            named(&format!("{REPOSITORY}/main/scenes/a%20b.usda")),
            Some(Named {
                item: format!("{REPOSITORY}/main"),
                path: "scenes/a%20b.usda".to_owned(),
            })
        );
        // A branch is one segment, which %2F may divide, and the top level has no path.
        assert_eq!(
            named(&format!("{REPOSITORY}/feature%2Fa/")),
            Some(Named {
                item: format!("{REPOSITORY}/feature%2Fa"),
                path: String::new(),
            })
        );
        assert_eq!(
            named(&format!("{REPOSITORY}/main")).unwrap().path,
            String::new()
        );
        assert_eq!(named("not-a-repository/main/a"), None);
        assert_eq!(named(REPOSITORY), None);
        assert_eq!(named(&format!("{REPOSITORY}/main/a/../b")), None);
        assert_eq!(named(&format!("{REPOSITORY}/main/a//b")), None);
        let (repository, branch) = item_parts(&format!("{REPOSITORY}/feature%2Fa")).unwrap();
        assert_eq!(repository.as_str(), REPOSITORY);
        assert_eq!(branch, "feature/a");
    }
}
