use veoveo_types::{ActionHandle, ActionName};

#[derive(veoveo_types::Vocabulary, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HubAction {
    ToolsList,
    ToolsCall,
    ResourcesList,
    ResourcesTemplatesList,
    ResourcesRead,
    SubscriptionsListen,
    PromptsList,
    PromptsGet,
    CompletionComplete,
    TasksGet,
    TasksUpdate,
    TasksCancel,
    // A request on a registered server's app origin.
    AppOrigin,
    AdminRead,
    AdminWrite,
}

impl HubAction {
    /// Actions on a registered server's tools, resources, prompts, Tasks and app origin.
    /// Installation roles grant them through the server's declared permissions; policy
    /// rules do not.
    pub fn targets_registered_server(self) -> bool {
        self.mcp_method().is_some() || self == Self::AppOrigin
    }

    pub fn mcp_method(self) -> Option<&'static str> {
        match self {
            Self::ToolsList => Some("tools/list"),
            Self::ToolsCall => Some("tools/call"),
            Self::ResourcesList => Some("resources/list"),
            Self::ResourcesTemplatesList => Some("resources/templates/list"),
            Self::ResourcesRead => Some("resources/read"),
            Self::SubscriptionsListen => Some("subscriptions/listen"),
            Self::PromptsList => Some("prompts/list"),
            Self::PromptsGet => Some("prompts/get"),
            Self::CompletionComplete => Some("completion/complete"),
            Self::TasksGet => Some("tasks/get"),
            Self::TasksUpdate => Some("tasks/update"),
            Self::TasksCancel => Some("tasks/cancel"),
            Self::AppOrigin | Self::AdminRead | Self::AdminWrite => None,
        }
    }
}

/// Runtime contributions retain their admitting registry identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyAction {
    Kernel(HubAction),
    Registered(ActionHandle),
}
impl PolicyAction {
    pub fn name(&self) -> ActionName {
        match self {
            Self::Kernel(action) => {
                ActionName::parse(action.as_str()).expect("closed kernel action spelling")
            }
            Self::Registered(action) => action.name().clone(),
        }
    }
    pub fn kernel(&self) -> Option<HubAction> {
        match self {
            Self::Kernel(action) => Some(*action),
            _ => None,
        }
    }
    pub fn registered(&self) -> Option<&ActionHandle> {
        match self {
            Self::Registered(action) => Some(action),
            _ => None,
        }
    }
    pub fn mcp_method(&self) -> Option<&'static str> {
        self.kernel().and_then(HubAction::mcp_method)
    }
}
impl From<HubAction> for PolicyAction {
    fn from(action: HubAction) -> Self {
        Self::Kernel(action)
    }
}
impl From<ActionHandle> for PolicyAction {
    fn from(action: ActionHandle) -> Self {
        Self::Registered(action)
    }
}
impl From<HubAction> for ActionName {
    fn from(action: HubAction) -> Self {
        PolicyAction::Kernel(action).name()
    }
}
