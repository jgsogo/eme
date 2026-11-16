use super::ToolError;
use crate::ToolContext;
use async_trait::async_trait;
use serde_json::Value;

/// An executable tool.
#[async_trait]
pub trait Tool: Send + Sync {
    async fn execute(&self, arguments: Value) -> Result<String, ToolError>;

    /// Executes this tool with trusted platform context. Implementations that
    /// access user-owned resources should override this method and scope every
    /// operation to `context.user_id`.
    async fn execute_with_context(
        &self,
        _context: &ToolContext,
        arguments: Value,
    ) -> Result<String, ToolError> {
        self.execute(arguments).await
    }
}
