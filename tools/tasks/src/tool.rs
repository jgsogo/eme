//! Tool adapters for creating, listing and removing scheduled agent tasks.

mod types;
pub(super) use types::{TaskAdd, TaskList, TaskRemove};

use agent_runtime::{Tool, ToolContext, ToolError};
use agent_service::{NewTask, TaskRegistry};
use async_trait::async_trait;
use serde_json::{Value, json};
use uuid::Uuid;

fn user(context: &ToolContext) -> Result<&str, ToolError> {
    context
        .user_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| ToolError::Execution("this operation requires an authenticated user".into()))
}
fn output(value: Value) -> Result<String, ToolError> {
    serde_json::to_string_pretty(&value).map_err(|e| ToolError::Execution(e.to_string()))
}

#[async_trait]
impl Tool for TaskList {
    async fn execute(&self, context: &ToolContext, _arguments: Value) -> Result<String, ToolError> {
        let tasks = self.0.list(user(context)?);
        output(json!({"tasks": tasks}))
    }
}

#[async_trait]
impl Tool for TaskAdd {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let new_task: NewTask = serde_json::from_value(arguments)
            .map_err(|e| ToolError::Execution(format!("invalid task definition: {e}")))?;
        let task = self
            .0
            .add(user(context)?, new_task)
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        output(json!({"task": task}))
    }
}

#[async_trait]
impl Tool for TaskRemove {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let id = arguments
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Execution("'id' must be a UUID string".into()))?;
        let id = Uuid::parse_str(id)
            .map_err(|e| ToolError::Execution(format!("invalid task id: {e}")))?;
        self.0
            .remove(user(context)?, id)
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        output(json!({"removed": id}))
    }
}
