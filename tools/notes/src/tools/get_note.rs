//! Tool that retrieves one note after checking the user's access.

use agent_runtime::{Tool, ToolContext, ToolDefinition, ToolError};
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::NotesStore;

pub struct GetNote(NotesStore);

impl GetNote {
    pub(super) fn new(store: NotesStore) -> Self {
        Self(store)
    }
}

pub(super) fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "get_note".into(),
        description:
            "Read the full content of one note by ID, only if it is accessible to the current user."
                .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "note_id": {
                    "type": "string",
                    "description": "ID returned by list_notes"
                }
            },
            "required": ["note_id"],
            "additionalProperties": false
        }),
    }
}

#[async_trait]
impl Tool for GetNote {
    async fn execute(&self, context: &ToolContext, args: Value) -> Result<String, ToolError> {
        let user = context
            .user_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                ToolError::Execution("notes require an authenticated user context".into())
            })?;
        let id = args
            .get("note_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| ToolError::Execution("'note_id' must be a non-empty string".into()))?;
        let note = self
            .0
            .get_accessible(user, None, id)
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        serde_json::to_string(&note).map_err(|error| ToolError::Execution(error.to_string()))
    }
}
