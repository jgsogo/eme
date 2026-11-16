use super::AgentListItem;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AgentCatalog {
    agents: std::sync::Arc<[AgentListItem]>,
}
