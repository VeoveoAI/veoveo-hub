use std::fmt;

use anyhow::Result;
use veoveo_hub_sdk::hub::HubToolName;
use veoveo_hub_sdk::{LocalToolName, ServerSlug};

use crate::HubCatalog;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HubToolProjection {
    pub server: ServerSlug,
    pub tool: LocalToolName,
}

impl HubToolProjection {
    pub fn new(server: ServerSlug, tool: LocalToolName) -> Self {
        Self { server, tool }
    }

    pub fn hub_name(&self) -> Result<HubToolName, HubNameError> {
        HubToolName::from_parts(&self.server, &self.tool)
            .map_err(HubNameError::InvalidProjectedToolName)
    }

    pub fn parse(name: &HubToolName) -> Result<Self, HubNameError> {
        let Some((server, tool)) = name.as_str().split_once("__") else {
            return Err(HubNameError::MissingNamespace(name.clone()));
        };
        if tool.contains("__") {
            return Err(HubNameError::InvalidNamespaceShape(name.clone()));
        }
        Ok(Self {
            server: ServerSlug::parse(server).map_err(HubNameError::InvalidServerSlug)?,
            tool: LocalToolName::parse(tool).map_err(HubNameError::InvalidLocalToolName)?,
        })
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
    pub fn project_tool_name(
        &self,
        server: &ServerSlug,
        tool: &LocalToolName,
    ) -> Result<HubToolName, HubNameError> {
        if self.server(server).is_none() {
            return Err(HubNameError::UnknownServer(server.clone()));
        }
        HubToolProjection::new(server.clone(), tool.clone()).hub_name()
    }

    pub fn parse_tool_name(&self, name: &HubToolName) -> Result<HubToolProjection, HubNameError> {
        let projection = HubToolProjection::parse(name)?;
        if self.server(&projection.server).is_none() {
            return Err(HubNameError::UnknownServer(projection.server));
        }
        Ok(projection)
    }
}
