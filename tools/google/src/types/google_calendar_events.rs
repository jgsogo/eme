use super::GoogleCalendarClient;
use crate::GoogleCalendarRuntimeConfig;
use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;

/// Read-only Google Calendar events tool, scoped to the authenticated EME user.
pub(crate) struct GoogleCalendarEvents {
    runtime: Option<GoogleCalendarRuntimeConfig>,
}

impl GoogleCalendarEvents {
    pub(crate) fn new(runtime: Option<GoogleCalendarRuntimeConfig>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl Tool for GoogleCalendarEvents {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let runtime = self.runtime.as_ref().ok_or_else(|| {
            ToolError::Configuration(
                "Google Calendar OAuth is not configured on this server.".into(),
            )
        })?;
        let user_id = context.user_id.as_deref().ok_or_else(|| {
            ToolError::Configuration("Google Calendar requires an authenticated EME user.".into())
        })?;
        let calendar_id = arguments
            .get("calendar_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool_name: "google_calendar_events".into(),
                errors: vec!["'calendar_id' must be an EME connected-calendar ID.".into()],
            })?;

        let date = arguments.get("date").and_then(Value::as_str);
        let time_min = arguments.get("time_min").and_then(Value::as_str);
        let time_max = arguments.get("time_max").and_then(Value::as_str);
        let time_zone = arguments.get("time_zone").and_then(Value::as_str);
        let has_date = date.is_some();
        let has_period = time_min.is_some() || time_max.is_some();
        if has_date == has_period || (has_period && (time_min.is_none() || time_max.is_none())) {
            return Err(ToolError::InvalidArguments {
                tool_name: "google_calendar_events".into(),
                errors: vec![
                    "Provide either 'date' or both 'time_min' and 'time_max', but not both.".into(),
                ],
            });
        }

        // Identity and family context are trusted host metadata. The model can
        // supply only the opaque EME calendar ID, never an owner or family ID.
        let family_id = context
            .family
            .as_ref()
            .map(|family| family.family_id.as_str());
        let authorized = runtime
            .access
            .authorize_read(user_id, calendar_id, family_id)
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;

        let token = runtime
            .refresh_access_token(authorized.calendar.connection_id, &authorized.refresh_token)
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        let client = GoogleCalendarClient::from_access_token(token.access_token);

        let public_calendar_id = authorized.calendar.id.to_string();
        let provider_calendar_id = authorized.calendar.provider_calendar_id.as_str();
        let result = if let Some(date) = date {
            client
                .events_for_day(
                    provider_calendar_id,
                    date,
                    time_zone.or(authorized.calendar.time_zone.as_deref()),
                    &public_calendar_id,
                )
                .await
        } else {
            client
                .events_for_period(
                    provider_calendar_id,
                    time_min.unwrap(),
                    time_max.unwrap(),
                    &public_calendar_id,
                )
                .await
        }
        .map_err(|error| ToolError::Execution(error.to_string()))?;
        serde_json::to_string_pretty(&result)
            .map_err(|error| ToolError::Execution(error.to_string()))
    }
}
