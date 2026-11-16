use super::ToolError;\nuse crate::ToolContext;
use async_trait::async_trait;
use serde_json::Value;

/// An executable tool.
#[async_trait]
pub trait Tool: Send + Sync {
    async fn execute(&self, arguments: Value) -> Result<String, ToolError>;\n\n    /// Executes this tool with trusted platform context. Implementations that\n    /// access user-owned resources should override this method and scope every\n    /// operation to `context.user_id`.\n    async fn execute_with_context(\n        &self,\n        _context: &ToolContext,\n        arguments: Value,\n    ) -> Result<String, ToolError> {\n        self.execute(arguments).await\n    }
}
