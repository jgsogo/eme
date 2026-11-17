//! Agent-facing free/busy lookup for a connected Google Calendar.
use crate::{GoogleCalendarRuntimeConfig, calendar::GoogleCalendarClient, refresh_access_token};
use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use chrono::DateTime;
use serde_json::{Value, json};

pub(crate) struct GoogleCalendarCheckAvailability {
    runtime: Option<GoogleCalendarRuntimeConfig>,
}

impl GoogleCalendarCheckAvailability {
    pub(crate) fn new(runtime: Option<GoogleCalendarRuntimeConfig>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl Tool for GoogleCalendarCheckAvailability {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let calendar_id = required_string(&arguments, "calendar_id")?;
        let time_min = required_string(&arguments, "time_min")?;
        let time_max = required_string(&arguments, "time_max")?;
        let start = DateTime::parse_from_rfc3339(time_min).map_err(|error| invalid(error.to_string()))?;
        let end = DateTime::parse_from_rfc3339(time_max).map_err(|error| invalid(error.to_string()))?;
        if start >= end {
            return Err(invalid("'time_min' must be earlier than 'time_max'".into()));
        }

        let runtime = self.runtime.as_ref().ok_or_else(|| {
            ToolError::Configuration("Google Calendar OAuth is not configured on this server.".into())
        })?;
        let user_id = context.user_id.as_deref().ok_or_else(|| {
            ToolError::Configuration("Google Calendar requires an authenticated EME user.".into())
        })?;
        let family_id = context.family.as_ref().map(|family| family.family_id.as_str());

        // Authorize every invocation before refreshing credentials.
        let authorized = runtime.access
            .authorize_read(user_id, calendar_id, family_id)
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        let token = refresh_access_token(&runtime.oauth, &authorized.refresh_token)
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        let client = GoogleCalendarClient::from_access_token(token.access_token);
        let calendar = authorized.calendar.id.to_string();
        let events = client.events_for_period(
            &authorized.calendar.provider_calendar_id,
            time_min,
            time_max,
            &calendar,
        ).await.map_err(|error| ToolError::Execution(error.to_string()))?;

        let busy = busy_intervals(&events);
        serde_json::to_string_pretty(&json!({
            "calendar_id": calendar,
            "time_min": time_min,
            "time_max": time_max,
            "available": busy.is_empty(),
            "busy": busy
        })).map_err(|error| ToolError::Execution(error.to_string()))
    }
}

fn required_string<'a>(arguments: &'a Value, name: &str) -> Result<&'a str, ToolError> {
    arguments.get(name).and_then(Value::as_str).map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(format!("'{name}' is required and must be a non-empty string")))
}

fn invalid(message: String) -> ToolError {
    ToolError::InvalidArguments {
        tool_name: "google_check_availability".into(),
        errors: vec![message],
    }
}

/// Return only busy intervals; never expose event titles, descriptions, attendees or IDs.
fn busy_intervals(events: &Value) -> Vec<Value> {
    events.get("events").and_then(Value::as_array)
        .into_iter().flatten()
        .filter(|event| event.get("status").and_then(Value::as_str) != Some("cancelled"))
        .filter(|event| event.get("transparency").and_then(Value::as_str) != Some("transparent"))
        .filter_map(|event| {
            let start = event.get("start")?;
            let end = event.get("end")?;
            let start_value = start.get("dateTime").or_else(|| start.get("date"))?.as_str()?;
            let end_value = end.get("dateTime").or_else(|| end.get("date"))?.as_str()?;
            Some(json!({"start": start_value, "end": end_value}))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn availability_exposes_only_busy_intervals() {
        let events = json!({"events":[
            {"id":"private-id","summary":"Private meeting","start":{"dateTime":"2026-11-18T10:00:00+01:00"},"end":{"dateTime":"2026-11-18T11:00:00+01:00"}},
            {"summary":"Free block","transparency":"transparent","start":{"dateTime":"2026-11-18T11:00:00+01:00"},"end":{"dateTime":"2026-11-18T12:00:00+01:00"}},
            {"status":"cancelled","start":{"dateTime":"2026-11-18T12:00:00+01:00"},"end":{"dateTime":"2026-11-18T13:00:00+01:00"}},
            {"start":{"date":"2026-11-19"},"end":{"date":"2026-11-20"}}
        ]});
        let busy = busy_intervals(&events);
        assert_eq!(busy.len(), 2);
        assert_eq!(busy[0]["start"], "2026-11-18T10:00:00+01:00");
        assert_eq!(busy[1]["start"], "2026-11-19");
        let serialized = serde_json::to_string(&busy).unwrap();
        assert!(!serialized.contains("Private meeting"));
        assert!(!serialized.contains("private-id"));
    }

    #[test]
    fn empty_busy_intervals_mean_available() {
        assert!(busy_intervals(&json!({"events":[]})).is_empty());
    }
}
