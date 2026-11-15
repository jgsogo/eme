use async_trait::async_trait;
use serde_json::Value;
use super::ToolError;

/// An executable tool.
#[async_trait]
pub trait Tool: Send + Sync {
    async fn execute(&self, arguments: Value) -> Result<String, ToolError>;
}

