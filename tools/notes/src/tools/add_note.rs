//! Tool that creates a private note for the authenticated user.

use agent_runtime::{Tool, ToolContext, ToolDefinition, ToolError};
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::{NewNote, NotesStore};

pub struct AddNote(NotesStore);

impl AddNote {
    pub(super) fn new(store: NotesStore) -> Self {
        Self(store)
    }
}

pub(super) fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "notes.add".into(),
        description: "Create a new private note for the current user. Use this to save information the user wants to remember for later. Notes created with this tool are private by default.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Short descriptive title for the note"
                },
                "content": {
                    "type": "string",
                    "description": "The note content to save"
                }
            },
            "required": ["title", "content"],
            "additionalProperties": false
        }),
    }
}

#[async_trait]
impl Tool for AddNote {
    async fn execute(&self, context: &ToolContext, args: Value) -> Result<String, ToolError> {
        let user = context
            .user_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                ToolError::Execution("notes require an authenticated user context".into())
            })?;
        let title = args
            .get("title")
            .and_then(Value::as_str)
            .filter(|title| !title.trim().is_empty())
            .ok_or_else(|| ToolError::Execution("'title' must be a non-empty string".into()))?;
        let content = args
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Execution("'content' must be a string".into()))?;

        let note = self
            .0
            .create(NewNote {
                owner_id: user.to_owned(),
                family_id: None,
                title: title.to_owned(),
                content: content.to_owned(),
            })
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        serde_json::to_string(&note).map_err(|error| ToolError::Execution(error.to_string()))
    }
}
