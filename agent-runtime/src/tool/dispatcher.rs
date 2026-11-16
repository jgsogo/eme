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
    async fn execute(&self, name: &str, arguments: Value) -> Result<String, ToolError>;\n\n    /// Executes a tool with trusted context supplied by the platform.\n    /// Existing dispatchers remain source-compatible and may override this.\n    async fn execute_with_context(\n        &self,\n        _context: &ToolContext,\n        name: &str,\n        arguments: Value,\n    ) -> Result<String, ToolError> {\n        self.execute(name, arguments).await\n    }
}
