use crate::{ToolContext, ToolDefinition, ToolError};
use async_trait::async_trait;
use serde_json::Value;

/// Executes tools requested by the model.
#[async_trait]
pub trait ToolDispatcher: Send + Sync {
    fn definition(&self, name: &str) -> Option<ToolDefinition> {
        let _ = name;
        None
    }
    async fn execute(&self, name: &str, arguments: Value) -> Result<String, ToolError>;

    /// Executes a tool with trusted context supplied by the platform.
    /// Existing dispatchers remain source-compatible and may override this.
    async fn execute_with_context(
        &self,
        _context: &ToolContext,
        name: &str,
        arguments: Value,
    ) -> Result<String, ToolError> {
        self.execute(name, arguments).await
    }
}
