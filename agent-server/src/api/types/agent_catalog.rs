use super::AgentListItem;
#[derive(Debug, Clone)]
pub struct AgentCatalog {
    pub(super) agents: std::sync::Arc<[AgentListItem]>,
}
