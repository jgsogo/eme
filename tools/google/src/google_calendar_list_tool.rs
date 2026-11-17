//! Agent-facing discovery of calendars the authenticated user may read.

use std::sync::Arc;

use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::GoogleCalendarRuntimeConfig;

/// Lists only selected calendars that are currently visible to the caller.
pub(crate) struct GoogleCalendarList {
    runtime: Option<GoogleCalendarRuntimeConfig>,
}

impl GoogleCalendarList {
    pub(crate) fn new(runtime: Option<GoogleCalendarRuntimeConfig>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl Tool for GoogleCalendarList {
    async fn execute(&self, context: &ToolContext, _arguments: Value) -> Result<String, ToolError> {
        let runtime = self.runtime.as_ref().ok_or_else(|| {
            ToolError::Configuration("Google Calendar OAuth is not configured on this server.".into())
        })?;
        let user_id = context.user_id.as_deref().ok_or_else(|| {
            ToolError::Configuration("Google Calendar requires an authenticated EME user.".into())
        })?;
        let family_id = context.family.as_ref().map(|family| family.family_id.as_str());
        let calendars = runtime
            .access
            .list_readable_calendars(user_id, family_id)
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;

        let calendars = calendars
            .into_iter()
            .map(|calendar| {
                let access = if calendar.owner_id.to_string() == user_id {
                    "owner"
                } else {
                    match calendar.family_access {
                        Some(agent_storage::CalendarFamilyAccess::Viewer) => "viewer",
                        Some(agent_storage::CalendarFamilyAccess::Contributor) => "contributor",
                        None => "unavailable",
                    }
                };
                json!({
                    "calendar_id": calendar.id.to_string(),
                    "name": calendar.display_name,
                    "time_zone": calendar.time_zone,
                    "access": access
                })
            })
            .collect::<Vec<_>>();

        serde_json::to_string_pretty(&json!({ "calendars": calendars }))
            .map_err(|error| ToolError::Execution(error.to_string()))
    }
}
