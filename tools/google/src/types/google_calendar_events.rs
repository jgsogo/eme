use super::GoogleCalendarClient;
use super::super::{is_calendar_allowed, required_env};
use agent_runtime::{Tool, ToolError};
use async_trait::async_trait;
use serde_json::Value;

/// Read-only Google Calendar events tool.
pub(crate) struct GoogleCalendarEvents;

#[async_trait]
impl Tool for GoogleCalendarEvents {
    async fn execute(&self, arguments: Value) -> Result<String, ToolError> {
        // An explicit tool argument takes precedence over the configured default.
        // There is deliberately no implicit fallback to Google's "primary" calendar.
        let configured_calendar_id = std::env::var("GOOGLE_CALENDAR_ID").ok();
        let calendar_id = arguments
            .get("calendar_id")
            .and_then(Value::as_str)
            .or(configured_calendar_id.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ToolError::Configuration(
                    "Set 'calendar_id' in the tool arguments or configure GOOGLE_CALENDAR_ID."
                        .into(),
                )
            })?;

        // Fail closed: only calendars explicitly listed in GOOGLE_CALENDAR_ALLOWED_IDS
        // can be queried. The special "primary" alias is never accepted.
        let allowed_calendar_ids = required_env("GOOGLE_CALENDAR_ALLOWED_IDS")
            .map_err(|error| ToolError::Configuration(error.to_string()))?;
        if !is_calendar_allowed(calendar_id, &allowed_calendar_ids) {
            return Err(ToolError::Configuration(format!(
                "Calendar '{calendar_id}' is not included in GOOGLE_CALENDAR_ALLOWED_IDS."
            )));
        }
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

        let client = GoogleCalendarClient::from_environment()
            .await
            .map_err(|error| ToolError::Configuration(error.to_string()))?;
        let result = if let Some(date) = date {
            client.events_for_day(calendar_id, date, time_zone).await
        } else {
            client
                .events_for_period(calendar_id, time_min.unwrap(), time_max.unwrap())
                .await
        }
        .map_err(|error| ToolError::Execution(error.to_string()))?;
        serde_json::to_string_pretty(&result)
            .map_err(|error| ToolError::Execution(error.to_string()))
    }
}
