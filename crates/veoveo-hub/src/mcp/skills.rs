//! MCP Skills (SEP-2640) through the Hub: `skills/list` gathers the skills of every server
//! the caller reaches, and for a Computer's own client the installation's own, and
//! `skills/get` answers one. A server's skills stay at `skill://{slug}/…`, so entries and
//! bytes pass unchanged and their digests hold; a skill is offered only to a caller who
//! may read every one of its files.
use futures::{StreamExt, stream};
use rmcp::{
    model::{
        CacheScope, ErrorData as McpError, GetSkillRequestParams, GetSkillResult, ListSkillsResult,
        PaginatedRequestParams, ReadResourceResult, Skill, SkillResources,
    },
    service::{RequestContext, RoleServer},
};
use veoveo_audit_contract::AuditOutcome;
use veoveo_hub_sdk::audit::{McpReadMethod, Read};
use veoveo_hub_sdk::hub::HubDiscoverySurface;
use veoveo_hub_sdk::skills::{SkillUri, owned_by};
use veoveo_hub_sdk::{HubDiscoveryMetadata, ServerSlug};

use super::{
    HubMcp,
    discovery::{
        AdmittedCatalog, DiscoveryCacheKey, MAX_CONCURRENT_DISCOVERY, enforce_complete_discovery,
        isolate_discovery_failures,
    },
    http_response::RequestError,
};
use crate::contract::HubAction;
use crate::control_plane::DiscoveryFailureMode;
use crate::{
    AuthenticatedSubject,
    mcp_support::{mcp_internal, mcp_invalid_params, resource_policy_target},
};

impl HubMcp {
    /// The installation's own skills when this operation's caller is a Computer's own
    /// client on the Computers' profile, and none for any other caller.
    fn own_skills(&self) -> Option<std::sync::Arc<crate::HubCatalog>> {
        let catalog = self.catalog.current();
        self.computers_of_caller(&catalog)?;
        (!catalog.hub_skills().is_empty()).then_some(catalog)
    }

    /// Lists the installation's own skills, for a Computer's own client, then those of
    /// every server the caller reaches that serves skills. A server that fails discovery
    /// is left out and named in the `ai.veoveo/hub` metadata, unless the profile requires
    /// complete discovery.
    pub(super) async fn handle_list_skills(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListSkillsResult, McpError> {
        let subject = self.authenticated(&context)?;
        let snapshot = self.catalog.snapshot();
        let catalog_generation = snapshot.generation();
        let catalog = snapshot.catalog();
        let discovery_failure_mode = catalog
            .profile(&self.profile_id)
            .map(|profile| profile.discovery_failure_mode)
            .unwrap_or(DiscoveryFailureMode::FailClosed);
        // A linked server's skills are one set, which the runner the caller reaches first
        // serves.
        let mut servers = Vec::<crate::registration::RegisteredServer>::new();
        for server in self.reachable(&subject, |capabilities| capabilities.skills) {
            let serving = match server.runner {
                None => Some(server),
                Some(_) => catalog
                    .shared_runner(&subject.principal, &server.slug)
                    .filter(|shared| shared.deployment == server.deployment)
                    .cloned(),
            };
            servers.extend(serving);
        }
        let fingerprint = super::discovery_authorization_fingerprint(&subject)?;
        let results = stream::iter(servers.into_iter().map(|server| {
            let context = &context;
            let subject = &subject;
            let hub = self.at(&server);
            let server_slug = server.slug.clone();
            async move {
                let key = DiscoveryCacheKey {
                    catalog_generation,
                    principal: subject.actor.id.clone(),
                    authorization_fingerprint: fingerprint,
                    server: server_slug.clone(),
                    link: server.link().cloned(),
                };
                let result = hub.server_skills(key, &server_slug, context, subject).await;
                (server_slug, result)
            }
        }))
        .buffer_unordered(MAX_CONCURRENT_DISCOVERY)
        .collect::<Vec<_>>()
        .await;
        let denied = results
            .iter()
            .filter_map(|(_, result)| result.as_ref().ok())
            .map(|catalog| catalog.denied)
            .sum();
        let results = results
            .into_iter()
            .map(|(server, result)| (server, result.map(|catalog| catalog.items)))
            .collect();
        let (served, degradation, errors) =
            isolate_discovery_failures(HubDiscoverySurface::Skills, results);
        for (server, error) in &errors {
            tracing::warn!(%server, %error, "isolated upstream skill discovery failure");
        }
        enforce_complete_discovery(HubDiscoverySurface::Skills, discovery_failure_mode, &errors)?;
        let mut skills = self
            .own_skills()
            .map(|catalog| catalog.hub_skills().skills().to_vec())
            .unwrap_or_default();
        skills.extend(served);
        self.record_discovery(
            &subject,
            veoveo_hub_sdk::audit::DiscoveryKind::Skills,
            skills.iter().map(|skill| skill.uri.clone()).collect(),
            denied,
        )
        .await?;
        let page = super::catalog_pages::page(
            skills,
            request.as_ref(),
            super::execution_composition(&subject)?,
        )?;
        Ok(ListSkillsResult {
            skills: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_hub_sdk::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(CacheScope::Private),
            meta: degradation.into_meta(),
        })
    }

    /// One server's admitted skills, from the discovery cache when present.
    async fn server_skills(
        &self,
        key: DiscoveryCacheKey,
        server_slug: &ServerSlug,
        context: &RequestContext<RoleServer>,
        subject: &AuthenticatedSubject,
    ) -> Result<AdmittedCatalog<Skill>, McpError> {
        self.ensure_discovery_watch(&key, context.peer.clone(), subject)
            .await?;
        if let Some(cached) = self.discovery.skills(&key).await {
            return Ok(cached);
        }
        let fetch = self.discovery.start_skills(key).await;
        let discovered = self
            .discover_skills_for_server(server_slug, context, subject)
            .await;
        match discovered {
            Ok(catalog) => {
                self.discovery
                    .store_skills(fetch, catalog.items.clone(), catalog.denied)
                    .await;
                Ok(catalog)
            }
            Err(error) => {
                if let Some(fetch) = fetch {
                    self.discovery
                        .finish_failure(HubDiscoverySurface::Skills, fetch)
                        .await;
                }
                Err(error)
            }
        }
    }

    /// The skills `server_slug` lists that the caller may read whole: each entry of the
    /// server's own, valid and with a full manifest, whose every file the caller's policy
    /// admits. Any other entry is left out and counted as denied.
    async fn discover_skills_for_server(
        &self,
        server_slug: &ServerSlug,
        context: &RequestContext<RoleServer>,
        subject: &AuthenticatedSubject,
    ) -> Result<AdmittedCatalog<Skill>, McpError> {
        let upstream = self
            .idempotent_upstream_request(
                server_slug,
                context.peer.clone(),
                subject,
                |upstream| async move { upstream.list_all_skills().await },
            )
            .await
            .map_err(RequestError::into_protocol)?;
        let mut items = Vec::with_capacity(upstream.len());
        let mut denied = 0u32;
        for skill in upstream {
            let targets = match skill_files(&skill, server_slug) {
                Some(files) => files
                    .iter()
                    .map(|uri| resource_policy_target(server_slug.clone(), uri))
                    .collect::<Result<Vec<_>, _>>()?,
                None => {
                    tracing::warn!(
                        server = %server_slug,
                        skill = %skill.uri,
                        "a server listed a skill that is not its own or not whole"
                    );
                    Vec::new()
                }
            };
            let allowed = !targets.is_empty()
                && self
                    .allows_catalog_targets(context, HubAction::SkillsList, targets)
                    .await?
                    .into_iter()
                    .all(|allowed| allowed);
            if allowed {
                items.push(skill);
            } else {
                denied = denied
                    .checked_add(1)
                    .ok_or_else(|| mcp_internal("catalog exceeds audit count range"))?;
            }
        }
        Ok(AdmittedCatalog { items, denied })
    }

    /// `skills/get`: the installation's own skill, for a Computer's own client, or the
    /// skill of the server whose slug its address names, when the caller may read every
    /// one of its files. Every answer and refusal is recorded.
    pub(super) async fn handle_get_skill(
        &self,
        request: GetSkillRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetSkillResult, RequestError> {
        let subject = self.authenticated(&context)?;
        let uri = SkillUri::parse(&request.uri)
            .ok()
            .filter(SkillUri::is_skill_md)
            .ok_or_else(|| mcp_invalid_params("no skill at this address"))?;
        let Some(server) = uri.server() else {
            let skill = self
                .own_skills()
                .and_then(|catalog| catalog.hub_skills().get(uri.as_str()).cloned());
            let result = skill
                .map(private_skill)
                .ok_or_else(|| mcp_invalid_params("no skill at this address").into());
            return self.finish_skill_get(&subject, &uri, result).await;
        };
        let routed = self
            .resource_server(&subject, HubAction::SkillsGet, uri.as_str())
            .await?;
        let hub = self.at(&routed);
        // The skill's own address first, so a caller who may not read it reaches no server.
        hub.admit_skill_get(&context, &subject, server.clone(), uri.as_str())
            .await?;
        let result = async {
            let got = hub
                .idempotent_upstream_request(&server, context.peer.clone(), &subject, |upstream| {
                    let request = request.clone();
                    async move { upstream.get_skill(request).await }
                })
                .await?;
            let files = skill_files(&got.skill, &server)
                .filter(|_| got.skill.uri == uri.as_str())
                .ok_or_else(|| mcp_internal("the server answered a skill that is not whole"))?;
            let targets = files
                .iter()
                .map(|file| resource_policy_target(server.clone(), file))
                .collect::<Result<Vec<_>, _>>()?;
            let allowed = hub
                .allows_catalog_targets(&context, HubAction::SkillsGet, targets)
                .await?
                .into_iter()
                .all(|allowed| allowed);
            if !allowed {
                return Err(RequestError::from(crate::mcp_support::mcp_invalid_request(
                    "you may not read every file of this skill",
                )));
            }
            Ok(private_skill(got.skill))
        }
        .await;
        // The admission above records its refusal before any request; this records the
        // answer and its outcome.
        hub.finish_skill_get(&subject, &uri, result).await
    }

    async fn finish_skill_get(
        &self,
        subject: &AuthenticatedSubject,
        uri: &SkillUri,
        result: Result<GetSkillResult, RequestError>,
    ) -> Result<GetSkillResult, RequestError> {
        let outcome = match &result {
            Ok(_) => AuditOutcome::Success,
            Err(error) if error.protocol().code == rmcp::model::ErrorCode::INVALID_REQUEST => {
                AuditOutcome::Denied
            }
            Err(_) => AuditOutcome::Failure,
        };
        self.record_skill_read(subject, McpReadMethod::SkillGet, uri, outcome)
            .await?;
        result
    }

    /// `resources/read` of one of the installation's own skill files, for a Computer's
    /// own client; any other caller is told the address is not exposed.
    pub(super) async fn read_own_skill_file(
        &self,
        uri: &SkillUri,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, RequestError> {
        let subject = self.authenticated(context)?;
        let read = self
            .own_skills()
            .and_then(|catalog| catalog.hub_skills().read(uri.as_str()));
        let (outcome, result) = match read {
            Some(mut result) => {
                result.ttl_ms = Some(0);
                result.cache_scope = Some(CacheScope::Private);
                (AuditOutcome::Success, Ok(result))
            }
            None => (
                AuditOutcome::Failure,
                Err(mcp_invalid_params(format!("resource URI is not exposed: {uri}")).into()),
            ),
        };
        self.record_skill_read(&subject, McpReadMethod::ResourceRead, uri, outcome)
            .await?;
        result
    }

    async fn record_skill_read(
        &self,
        subject: &AuthenticatedSubject,
        method: McpReadMethod,
        uri: &SkillUri,
        outcome: AuditOutcome,
    ) -> Result<(), McpError> {
        let detail = crate::audit::hub_audit()
            .detail(&Read {
                method,
                invocation: Some(subject.audit_invocation()),
            })
            .map_err(|_| mcp_internal("invalid skill read audit detail"))?;
        let target = crate::audit::resource_target(uri.resource_uri())
            .map_err(|_| mcp_internal("invalid skill read audit target"))?;
        let draft = subject
            .audit_draft(detail, outcome)
            .target(target)
            .build()
            .map_err(|_| mcp_internal("invalid skill read audit attribution"))?;
        self.state
            .record_audit(draft)
            .await
            .map_err(|_| mcp_internal("required skill read audit unavailable"))
    }
}

/// Every file address of `skill` when it is a valid skill of `server` with a full
/// manifest, all of its files the server's own; `None` for any other entry.
fn skill_files(skill: &Skill, server: &ServerSlug) -> Option<Vec<String>> {
    if skill.validate().is_err() || !owned_by(&skill.uri, server) {
        return None;
    }
    let SkillResources::Files(files) = &skill.resources else {
        return None;
    };
    files
        .iter()
        .map(|file| owned_by(&file.uri, server).then(|| file.uri.clone()))
        .collect()
}

/// `skill` as one caller's answer: every get rechecks authority, so no cache keeps it.
fn private_skill(skill: Skill) -> GetSkillResult {
    GetSkillResult::new(skill)
        .with_ttl_ms(0)
        .with_cache_scope(CacheScope::Private)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{SkillFrontmatter, SkillResource};

    const DIGEST: &str = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    fn skill(uri: &str, files: &[&str]) -> Skill {
        Skill::new(
            uri,
            SkillFrontmatter::new("read-readings", "Read readings."),
            SkillResources::Files(
                files
                    .iter()
                    .map(|file| SkillResource::new(*file, DIGEST, 3))
                    .collect(),
            ),
        )
    }

    #[test]
    fn a_server_offers_only_its_own_whole_skills() {
        let observatory = ServerSlug::parse("observatory").unwrap();
        let own = skill(
            "skill://observatory/read-readings/SKILL.md",
            &[
                "skill://observatory/read-readings/SKILL.md",
                "skill://observatory/read-readings/references/units.md",
            ],
        );
        assert_eq!(
            skill_files(&own, &observatory).unwrap(),
            [
                "skill://observatory/read-readings/SKILL.md",
                "skill://observatory/read-readings/references/units.md"
            ]
        );
        // Another server's address, the Hub's own, or an entry that is not whole.
        for foreign in [
            skill(
                "skill://time/read-readings/SKILL.md",
                &["skill://time/read-readings/SKILL.md"],
            ),
            skill(
                "skill://veoveo/read-readings/SKILL.md",
                &["skill://veoveo/read-readings/SKILL.md"],
            ),
            skill("skill://observatory/read-readings/SKILL.md", &[]),
            Skill::new(
                "skill://observatory/read-readings/SKILL.md",
                SkillFrontmatter::new("read-readings", "Read readings."),
                SkillResources::Dynamic,
            ),
        ] {
            assert!(skill_files(&foreign, &observatory).is_none(), "{foreign:?}");
        }
    }

    #[test]
    fn a_skill_answer_is_private_to_its_caller() {
        let answer = private_skill(skill(
            "skill://observatory/read-readings/SKILL.md",
            &["skill://observatory/read-readings/SKILL.md"],
        ));
        assert_eq!(answer.ttl_ms, Some(0));
        assert_eq!(answer.cache_scope, Some(CacheScope::Private));
    }
}
