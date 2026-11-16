use crate::{ToolContext, ToolDefinition, ToolError};
use async_trait::async_trait;
use serde_json::Value;

/// Executes tools requested by the model.
#[async_trait]
pub trait ToolDispatcher: Send + Sync {
    fn definition(&self, name: &str) -> Option<ToolDefinition>;
    async fn execute(
        &self,
        context: &ToolContext,
        name: &str,
        arguments: Value,
    ) -> Result<String, ToolError>;
}
