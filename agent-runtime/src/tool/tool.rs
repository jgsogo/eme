use super::ToolError;
use async_trait::async_trait;
use serde_json::Value;

/// An executable tool.
#[async_trait]
pub trait Tool: Send + Sync {
    async fn execute(&self, arguments: Value) -> Result<String, ToolError>;
}
