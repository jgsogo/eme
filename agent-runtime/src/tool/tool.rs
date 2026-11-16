//! Executable tool contract implemented by integration crates.

use super::ToolError;
use crate::ToolContext;
use async_trait::async_trait;
use serde_json::Value;

/// An executable tool.
#[async_trait]
pub trait Tool: Send + Sync {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError>;
}
