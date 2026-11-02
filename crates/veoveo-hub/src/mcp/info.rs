use std::{collections::BTreeSet, fmt::Write as _};

use anyhow::{Context as _, Result};
use rmcp::{
    ErrorData as McpError,
    handler::server::ServerHandler,
    model::{
        DiscoverResult, ExtensionCapabilities, Implementation, JsonObject, ServerCapabilities,
        ServerConfig,
    },
    service::{RequestContext, RoleServer},
};
use veoveo_types::RoleId;

use super::HubMcp;
use crate::{
    AuthenticatedSubject, HubCatalog,
    computers::{ComputerId, ComputerVersion},
    control_plane::Computers,
    person::{Person, PersonGroup},
};

impl HubMcp {
    fn auth_extension_capabilities(&self) -> Option<ExtensionCapabilities> {
        let catalog = self.catalog.current();
        let profile = catalog.profile(&self.profile_id)?;
        let extensions: ExtensionCapabilities = profile
            .auth_modes
            .iter()
            .filter_map(|mode| mode.mcp_extension_id())
            .map(|extension| (extension.to_string(), JsonObject::new()))
            .collect();
        if extensions.is_empty() {
            None
        } else {
            Some(extensions)
        }
    }

    /// Servers register and leave while a session lasts, so the Hub advertises
    /// every surface it routes and announces list changes for each of them.
    pub(super) fn handle_get_info(&self) -> ServerConfig {
        let mut capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_tool_list_changed()
            .enable_resources()
            .enable_resources_list_changed()
            .enable_resources_subscribe()
            .enable_prompts()
            .enable_prompts_list_changed()
            .enable_completions()
            .build();
        let mut extensions = self.auth_extension_capabilities().unwrap_or_default();
        extensions.insert(
            rmcp::model::TASKS_EXTENSION_ID.to_owned(),
            JsonObject::new(),
        );
        capabilities.extensions = Some(extensions);

        let mut info = ServerConfig::default();
        info.capabilities = capabilities;
        info.server_info = Implementation::new("veoveo-hub", env!("CARGO_PKG_VERSION"));
        let mut instructions = HUB_INSTRUCTIONS.to_string();
        if let Some(handbook) = self.computer_handbook() {
            instructions.push_str("\n\n");
            instructions.push_str(&handbook);
        }
        info.instructions = Some(instructions);
        info
    }

    /// `server/discover`'s answer, for the operation's authenticated caller.
    pub(super) async fn handle_discover(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<DiscoverResult, McpError> {
        let subject = self.authenticated(context).ok();
        Ok(DiscoverResult::from_server_info(
            self.supported_protocol_versions().into_owned(),
            self.discovered_info(subject.as_ref()).await,
        ))
    }

    /// The server information `server/discover` answers `subject`: `get_info`'s, and for
    /// a Computer's own client, after its handbook, the section about the person whose
    /// Computer it is.
    async fn discovered_info(&self, subject: Option<&AuthenticatedSubject>) -> ServerConfig {
        let mut info = self.handle_get_info();
        let section = match subject {
            Some(subject) => self.person_section(subject).await,
            None => None,
        };
        if let Some(section) = section {
            let instructions = info.instructions.get_or_insert_with(String::new);
            instructions.push_str("\n\n");
            instructions.push_str(&section);
        }
        info
    }

    /// The installation's Computer settings in `catalog`, when this operation's caller is
    /// a Computer's own client on the Computers' profile: no other caller of the Hub gets
    /// what follows from them.
    fn computers_of_caller<'a>(&self, catalog: &'a HubCatalog) -> Option<&'a Computers> {
        let client = self.client.as_ref()?;
        let computers = catalog.control_plane().computers.as_ref()?;
        (computers.profile == self.profile_id && &computers.oauth_client == client)
            .then_some(computers)
    }

    /// The installation's Computer handbook, for a Computer's own client on the
    /// Computers' profile only: other callers of the Hub never receive it.
    fn computer_handbook(&self) -> Option<String> {
        let catalog = self.catalog.current();
        self.computers_of_caller(&catalog)?
            .handbook
            .as_deref()
            .map(str::trim)
            .filter(|handbook| !handbook.is_empty())
            .map(str::to_owned)
    }

    /// The section about the person whose Computer `subject`'s client is, for a
    /// Computer's own client only. A record that cannot be read leaves it out, so the
    /// Computer still connects.
    async fn person_section(&self, subject: &AuthenticatedSubject) -> Option<String> {
        let catalog = self.catalog.current();
        let computers = self.computers_of_caller(&catalog)?;
        match self.person_brief(&catalog, computers, subject).await {
            Ok(brief) => brief.map(|brief| brief.markdown()),
            Err(error) => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    "discovery answers a Computer without the section about its person"
                );
                None
            }
        }
    }

    /// What the Hub's own records say about `subject`'s person and their Computer; none
    /// when they own no Computer.
    async fn person_brief(
        &self,
        catalog: &HubCatalog,
        computers: &Computers,
        subject: &AuthenticatedSubject,
    ) -> Result<Option<PersonBrief>> {
        let principal = &subject.principal;
        let Some(tenant) = &principal.tenant else {
            return Ok(None);
        };
        let store = self.state.hub_store();
        let Some(computer) = store
            .owned_computer(tenant, &principal.id)
            .await
            .context("read the caller's Computer")?
        else {
            return Ok(None);
        };
        let person = store
            .people(tenant, std::slice::from_ref(&principal.id))
            .await?
            .into_iter()
            .next();
        let mut groups = match &self.state.person_groups {
            Some(groups) => groups
                .groups(catalog, tenant, &principal.id)
                .context("name the person's groups")?,
            None => Vec::new(),
        };
        groups.sort();
        Ok(Some(PersonBrief {
            name: person
                .as_ref()
                .map(|person| person.name.as_str().to_owned())
                .or_else(|| {
                    subject
                        .principal_display_name
                        .as_ref()
                        .map(|name| name.as_str().to_owned())
                })
                .unwrap_or_else(|| principal.id.as_str().to_owned()),
            person,
            principal: principal.id.as_str().to_owned(),
            tenant: titled(
                tenant.as_str(),
                catalog.tenant(tenant).and_then(|t| t.title.as_deref()),
            ),
            work_context: titled(
                computers.work_context.as_str(),
                catalog
                    .work_context(&computers.work_context)
                    .map(|context| context.title.as_str()),
            ),
            installation_roles: crate::policy::held_roles(
                &catalog.control_plane().authorization,
                principal,
            ),
            sign_in_roles: principal.roles.clone(),
            groups,
            computer: computer.id.clone(),
            version: computer
                .provisioning
                .version()
                .context("name the Computer's version")?,
        }))
    }
}

/// `id` with its title before it, when it has one.
fn titled(id: &str, title: Option<&str>) -> String {
    match title.map(str::trim).filter(|title| !title.is_empty()) {
        Some(title) => format!("{title} (`{id}`)"),
        None => format!("`{id}`"),
    }
}

/// The person whose Computer a client is, as the Hub's own records name them.
struct PersonBrief {
    /// The name to call them by: their last sign-in's, else their principal's.
    name: String,
    person: Option<Person>,
    principal: String,
    tenant: String,
    work_context: String,
    installation_roles: BTreeSet<RoleId>,
    sign_in_roles: BTreeSet<RoleId>,
    groups: Vec<PersonGroup>,
    computer: ComputerId,
    version: ComputerVersion,
}

impl PersonBrief {
    fn markdown(&self) -> String {
        let roles = |roles: &BTreeSet<RoleId>| {
            roles
                .iter()
                .map(|role| format!("`{}`", role.as_str()))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut text = String::from("## Who you work for\n\n");
        let _ = write!(text, "You work for {}", self.name);
        if let Some(email) = self
            .person
            .as_ref()
            .and_then(|person| person.email.as_ref())
        {
            let _ = write!(text, ", {email}");
        }
        let _ = writeln!(
            text,
            ". Their principal is `{}` in the tenant {}. This Computer works in the Work Context {}.",
            self.principal, self.tenant, self.work_context
        );
        if !self.installation_roles.is_empty() {
            let _ = writeln!(
                text,
                "\nTheir installation roles: {}.",
                roles(&self.installation_roles)
            );
        }
        if !self.sign_in_roles.is_empty() {
            let _ = writeln!(
                text,
                "\nThe roles their sign-in carries: {}.",
                roles(&self.sign_in_roles)
            );
        }
        if self.groups.is_empty() {
            text.push_str("\nThey belong to no group.\n");
        } else {
            text.push_str("\nTheir groups:\n");
            for group in &self.groups {
                let _ = writeln!(text, "- `{}`, {}", group.name, group.role);
            }
        }
        let _ = write!(
            text,
            "\nThis is their Computer `{}`, at version `{}`. Everything here acts as {}: every Hub tool, and every command line that holds this Computer's credentials.",
            self.computer, self.version, self.name
        );
        text
    }
}

const HUB_INSTRUCTIONS: &str = "Veoveo Hub for registered MCP servers. Tool names are namespaced at this MCP surface; resource URIs remain server-owned.";

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use rmcp::handler::server::ServerHandler;
    use veoveo_hub_sdk::{HubProfileId, OAuthClientId};
    use veoveo_types::{PrincipalId, TenantId};

    use super::HUB_INSTRUCTIONS;
    use crate::{
        HubCatalog, HubCatalogHandle,
        contract::PrincipalDisplayName,
        control_plane::HubControlPlane,
        mcp::HubMcp,
        person::{Email, Person, PersonGroup, PersonGroupRole},
    };

    const HANDBOOK: &str =
        "## Home\n\n`~` is always kept; everything else comes with the Computer version.\n";

    fn plane(handbook: Option<&str>) -> HubControlPlane {
        let mut plane: HubControlPlane =
            serde_json::from_str(include_str!("../../testdata/hub.json")).unwrap();
        plane.secrets.push(
            serde_json::from_value(serde_json::json!({
                "id": "computer-model-api-key", "source": "env", "purpose": "provider_api_key",
                "locator": "UNUSED_MODEL_API_KEY", "owner": {"kind": "hub"}, "metadata": {}
            }))
            .unwrap(),
        );
        let mut computers: crate::control_plane::Computers = serde_json::from_value(serde_json::json!({
            "gateway": {"endpoint": "https://gateway.example/", "certificateAuthority": "/unused/ca", "bearerEnvironment": "UNUSED"},
            "template": {"workspace": "installation", "name": "computer"},
            "policyFile": "/unused/policy", "command": ["/usr/local/bin/opencode", "serve"],
            "oauthClient": "operator-local-public", "profile": "operator", "workContext": "operations", "port": 4096,
            "hubProvider": {"profile": "vv-computer-hub", "credentialKey": "COMPUTER_HUB_REFERENCE"},
            "modelProvider": {"profile": "vv-computer-model", "credentialKey": "DEEPINFRA_API_KEY", "secret": "computer-model-api-key"},
            "loreProvider": {"profile": "vv-lore", "credentialKey": "LORE_ACCESS_TOKEN"}
        }))
        .unwrap();
        computers.handbook = handbook.map(str::to_owned);
        plane.computers = Some(computers);
        plane
    }

    /// The Hub as an operation of a caller of `client` on `profile` sees it.
    fn hub(
        state: &crate::HubState,
        plane: HubControlPlane,
        profile: &str,
        client: Option<&str>,
    ) -> HubMcp {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let catalog = HubCatalogHandle::new(Arc::new(
            HubCatalog::from_control_plane(plane, crate::test_catalog::binding()).unwrap(),
        ));
        HubMcp {
            client: client.map(|client| OAuthClientId::parse(client).unwrap()),
            ..HubMcp::new(
                catalog,
                HubProfileId::parse(profile).unwrap(),
                state.clone(),
                crate::test_catalog::token_issuer(),
                Default::default(),
            )
        }
    }

    /// The instructions `get_info` gives a caller of `client` on `profile`.
    fn instructions(
        state: &crate::HubState,
        plane: HubControlPlane,
        profile: &str,
        client: Option<&str>,
    ) -> String {
        hub(state, plane, profile, client)
            .get_info()
            .instructions
            .unwrap()
    }

    #[tokio::test]
    async fn computer_clients_receive_the_handbook_as_server_instructions() {
        tokio::time::timeout(Duration::from_secs(90), async {
            let db = crate::test_store::TestDb::new().await;
            let state = crate::HubState::new(db.a.clone());
            let computer = instructions(
                &state,
                plane(Some(HANDBOOK)),
                "operator",
                Some("operator-local-public"),
            );
            assert_eq!(
                computer,
                format!("{HUB_INSTRUCTIONS}\n\n{}", HANDBOOK.trim())
            );
            for (profile, client) in [
                // Another client of the Computers' profile.
                ("operator", Some("operator-delegated")),
                // The Computers' client on another profile.
                ("admin", Some("operator-local-public")),
                ("operator", None),
            ] {
                assert_eq!(
                    instructions(&state, plane(Some(HANDBOOK)), profile, client),
                    HUB_INSTRUCTIONS,
                    "{profile} {client:?} must not receive the Computer handbook"
                );
            }
            for handbook in [None, Some(""), Some(" \n")] {
                assert_eq!(
                    instructions(
                        &state,
                        plane(handbook),
                        "operator",
                        Some("operator-local-public")
                    ),
                    HUB_INSTRUCTIONS
                );
            }
        })
        .await
        .expect("instructions fixture deadline");
    }

    /// The groups a contribution names for the test's person: one they own, one they
    /// are a member of.
    #[derive(Debug)]
    struct Groups;

    impl crate::person::PersonGroups for Groups {
        fn groups(
            &self,
            _: &HubCatalog,
            tenant: &TenantId,
            principal: &PrincipalId,
        ) -> anyhow::Result<Vec<PersonGroup>> {
            assert_eq!(
                (tenant.as_str(), principal.as_str()),
                ("tenant-a", "issuer#subject")
            );
            Ok(vec![
                PersonGroup {
                    name: "ops".to_owned(),
                    role: PersonGroupRole::Member,
                },
                PersonGroup {
                    name: "flight".to_owned(),
                    role: PersonGroupRole::Owner,
                },
            ])
        }
    }

    #[tokio::test]
    async fn a_computer_client_discovers_whom_it_works_for_after_the_handbook() {
        tokio::time::timeout(Duration::from_secs(90), async {
            let db = crate::test_store::TestDb::new().await;
            let state = crate::HubState::new(db.a.clone())
                .bind_person_groups(Arc::new(Groups))
                .unwrap();
            let mut plane = plane(Some(HANDBOOK));
            plane.authorization = serde_json::from_value(serde_json::json!({
                "roles": [{"name": "member", "access": ["read", "write"]}],
                "grants": [{"role": "member", "subject": {"principal": "issuer#subject"}}]
            }))
            .unwrap();
            let tenant = TenantId::parse("tenant-a").unwrap();
            let mut subject = crate::mcp::task_ownership_tests::subject();
            subject.principal.tenant = Some(tenant.clone());
            let discovered = |hub: HubMcp| {
                let subject = subject.clone();
                async move {
                    hub.discovered_info(Some(&subject))
                        .await
                        .instructions
                        .unwrap()
                }
            };
            let computer_client = || {
                hub(
                    &state,
                    plane.clone(),
                    "operator",
                    Some("operator-local-public"),
                )
            };
            let handbook = format!("{HUB_INSTRUCTIONS}\n\n{}", HANDBOOK.trim());

            // A person without a Computer is told about no Computer.
            assert_eq!(discovered(computer_client()).await, handbook);

            state
                .hub_store()
                .record_person(
                    &Person {
                        tenant: tenant.clone(),
                        principal: subject.principal.id.clone(),
                        name: PrincipalDisplayName::new("Alex Rozgo").unwrap(),
                        email: Some(Email::parse("alex@example.com").unwrap()),
                    },
                    chrono::Utc::now(),
                )
                .await
                .unwrap();
            let computer = state
                .hub_store()
                .ensure_computer(
                    &tenant,
                    &subject.principal.id,
                    &crate::computers::ComputerProvisionSpec::fixture(
                        plane.computers.clone().unwrap(),
                    ),
                )
                .await
                .unwrap();
            let version = computer.provisioning.version().unwrap();
            assert_eq!(
                discovered(computer_client()).await,
                format!(
                    "{handbook}\n\n## Who you work for\n\n\
                     You work for Alex Rozgo, alex@example.com. Their principal is \
                     `issuer#subject` in the tenant Smoke tenant (`tenant-a`). This Computer \
                     works in the Work Context Operations (`operations`).\n\n\
                     Their installation roles: `member`.\n\n\
                     The roles their sign-in carries: `operator`.\n\n\
                     Their groups:\n- `flight`, owner\n- `ops`, member\n\n\
                     This is their Computer `{}`, at version `{version}`. Everything here \
                     acts as Alex Rozgo: every Hub tool, and every command line that holds \
                     this Computer's credentials.",
                    computer.id
                )
            );

            // No other caller learns about the person, nor does a caller without a subject.
            for (profile, client) in [
                ("operator", Some("operator-delegated")),
                ("admin", Some("operator-local-public")),
                ("operator", None),
            ] {
                assert_eq!(
                    discovered(hub(&state, plane.clone(), profile, client)).await,
                    HUB_INSTRUCTIONS,
                    "{profile} {client:?}"
                );
            }
            assert_eq!(
                computer_client()
                    .discovered_info(None)
                    .await
                    .instructions
                    .unwrap(),
                handbook
            );
        })
        .await
        .expect("person section fixture deadline");
    }
}
