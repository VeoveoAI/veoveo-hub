use std::fmt;

use anyhow::Result;
use veoveo_hub_sdk::hub::{HubToolName, HubToolParts, LinkName};
use veoveo_hub_sdk::{LocalToolName, ServerSlug};

use crate::HubCatalog;
use crate::registration::RegisteredServer;

/// A tool as the Hub names it: its server, the link whose runner serves it when the
/// server is linked, and its name on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HubToolProjection {
    pub server: ServerSlug,
    pub link: Option<LinkName>,
    pub tool: LocalToolName,
}

impl HubToolProjection {
    pub fn new(server: ServerSlug, tool: LocalToolName) -> Self {
        Self {
            server,
            link: None,
            tool,
        }
    }

    /// `tool` as the deployment `server` serves it: under its link when it is a runner.
    pub fn of(server: &RegisteredServer, tool: LocalToolName) -> Self {
        Self {
            server: server.slug.clone(),
            link: server.link().cloned(),
            tool,
        }
    }

    /// `{slug}__{tool}`, or `{slug}__{link}__{tool}` for a runner's tool.
    pub fn hub_name(&self) -> Result<HubToolName, HubNameError> {
        match &self.link {
            None => HubToolName::from_parts(&self.server, &self.tool),
            Some(link) => HubToolName::from_linked_parts(&self.server, link, &self.tool),
        }
        .map_err(HubNameError::InvalidProjectedToolName)
    }

    pub fn parse(name: &HubToolName) -> Result<Self, HubNameError> {
        let HubToolParts { server, link, tool } = name.projection().map_err(|error| {
            use veoveo_hub_sdk::hub::HubToolNamespaceError as Namespace;
            match error {
                Namespace::Missing => HubNameError::MissingNamespace(name.clone()),
                Namespace::Server(error) => HubNameError::InvalidServerSlug(error),
                Namespace::Tool(error) => HubNameError::InvalidLocalToolName(error),
                _ => HubNameError::InvalidNamespaceShape(name.clone()),
            }
        })?;
        Ok(Self { server, link, tool })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubNameError {
    UnknownServer(ServerSlug),
    MissingNamespace(HubToolName),
    InvalidNamespaceShape(HubToolName),
    InvalidServerSlug(veoveo_types::IdentifierError),
    InvalidLocalToolName(veoveo_types::IdentifierError),
    InvalidProjectedToolName(veoveo_hub_sdk::hub::HubToolNameError),
}

impl fmt::Display for HubNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownServer(server) => write!(f, "unknown server `{server}`"),
            Self::MissingNamespace(name) => {
                write!(f, "hub tool `{name}` is missing server namespace")
            }
            Self::InvalidNamespaceShape(name) => {
                write!(f, "hub tool `{name}` has an invalid namespace shape")
            }
            Self::InvalidServerSlug(err) | Self::InvalidLocalToolName(err) => err.fmt(f),
            Self::InvalidProjectedToolName(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for HubNameError {}

impl HubCatalog {
    /// The name of `tool` as `server` serves it: under its link when it is a runner.
    pub fn project_tool_name(
        &self,
        server: &RegisteredServer,
        tool: &LocalToolName,
    ) -> Result<HubToolName, HubNameError> {
        HubToolProjection::of(server, tool.clone()).hub_name()
    }

    /// The server, link and tool `name` names. A server that is not registered is
    /// unknown; whether the name, with its link or without, reaches a runner the caller
    /// may use is decided where the call routes.
    pub fn parse_tool_name(&self, name: &HubToolName) -> Result<HubToolProjection, HubNameError> {
        let projection = HubToolProjection::parse(name)?;
        let registered = self.server(&projection.server).is_some()
            || self
                .registrations()
                .runners(&projection.server)
                .next()
                .is_some();
        if !registered {
            return Err(HubNameError::UnknownServer(projection.server));
        }
        Ok(projection)
    }
}
