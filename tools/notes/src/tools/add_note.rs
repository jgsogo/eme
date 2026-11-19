//! Tool that creates a note for the authenticated user, optionally shared with a family.

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
        description: "Create a note for the current user. Notes are private by default. To share a note with a family, provide that family's ID as family_id; the user must be a member of that family.".into(),
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
                },
                "family_id": {
                    "type": "string",
                    "description": "Optional ID of a family to share this note with. Omit to keep the note private."
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
        let family_id = match args.get("family_id") {
            None | Some(Value::Null) => None,
            Some(Value::String(id)) if !id.trim().is_empty() => Some(id.trim().to_owned()),
            Some(_) => {
                return Err(ToolError::Execution(
                    "'family_id' must be a non-empty string when provided".into(),
                ));
            }
        };

        let note = self
            .0
            .create(NewNote {
                owner_id: user.to_owned(),
                family_id,
                title: title.to_owned(),
                content: content.to_owned(),
            })
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        serde_json::to_string(&note).map_err(|error| ToolError::Execution(error.to_string()))
    }
}
