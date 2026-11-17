//! Tool that lists note metadata visible to the authenticated user.

use agent_runtime::{Tool, ToolContext, ToolDefinition, ToolError};
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::NotesStore;

pub struct ListNotes(NotesStore);

impl ListNotes {
    pub(super) fn new(store: NotesStore) -> Self {
        Self(store)
    }
}

pub(super) fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "list_notes".into(),
        description: "List titles and metadata of notes accessible to the current user. Note contents are not returned.".into(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
    }
}

#[async_trait]
impl Tool for ListNotes {
    async fn execute(&self, context: &ToolContext, _: Value) -> Result<String, ToolError> {
        let user = context
            .user_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                ToolError::Execution("notes require an authenticated user context".into())
            })?;
        let notes = self
            .0
            .list_accessible(user, None)
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        serde_json::to_string(&notes).map_err(|error| ToolError::Execution(error.to_string()))
    }
}
