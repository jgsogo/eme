use async_trait::async_trait;
use serde_json::Value;
use crate::{ToolDefinition, ToolError};

/// Executes tools requested by the model.
#[async_trait]
pub trait ToolDispatcher: Send + Sync {
    fn definition(&self, name: &str) -> Option<ToolDefinition> {
        let _ = name;
        None
    }
    async fn execute(&self, name: &str, arguments: Value) -> Result<String, ToolError>;
}

