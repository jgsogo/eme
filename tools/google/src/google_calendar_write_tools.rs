//! Agent-facing Google Calendar event creation and update tools.
use crate::{GoogleCalendarRuntimeConfig, calendar::GoogleCalendarClient};
use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use chrono::DateTime;
use serde_json::{Map, Value, json};

pub(crate) struct GoogleCalendarCreateEvent {
    runtime: Option<GoogleCalendarRuntimeConfig>,
}
impl GoogleCalendarCreateEvent {
    pub(crate) fn new(runtime: Option<GoogleCalendarRuntimeConfig>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl Tool for GoogleCalendarCreateEvent {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let calendar_id =
            required_string(&arguments, "calendar_id", "google_calendar_create_event")?;
        let summary = required_string(&arguments, "summary", "google_calendar_create_event")?;
        let start = required_datetime(&arguments, "start", "google_calendar_create_event")?;
        let end = required_datetime(&arguments, "end", "google_calendar_create_event")?;
        if parse_datetime(start, "start", "google_calendar_create_event")?
            >= parse_datetime(end, "end", "google_calendar_create_event")?
        {
            return invalid(
                "google_calendar_create_event",
                "'start' must be earlier than 'end'",
            );
        }
        let mut event = Map::new();
        event.insert("summary".into(), Value::String(summary.to_owned()));
        event.insert("start".into(), json!({"dateTime": start}));
        event.insert("end".into(), json!({"dateTime": end}));
        add_optional_text(
            &mut event,
            &arguments,
            "description",
            8192,
            "google_calendar_create_event",
        )?;
        add_optional_text(
            &mut event,
            &arguments,
            "location",
            1024,
            "google_calendar_create_event",
        )?;

        let (client, authorized, public_id) =
            authorized_client(self.runtime.as_ref(), context, calendar_id).await?;
        let created = client
            .create_event(
                &authorized.calendar.provider_calendar_id,
                Value::Object(event),
            )
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        tracing::info!(
            operation = "create",
            user_id = context.user_id.as_deref().unwrap_or("unknown"),
            calendar_id = %public_id,
            event_id = created.get("id").and_then(|value| value.as_str()).unwrap_or("unknown"),
            "Google Calendar event write completed"
        );
        serde_json::to_string_pretty(
            &json!({"calendar_id": public_id, "event": public_event(created)}),
        )
        .map_err(|error| ToolError::Execution(error.to_string()))
    }
}

pub(crate) struct GoogleCalendarUpdateEvent {
    runtime: Option<GoogleCalendarRuntimeConfig>,
}
impl GoogleCalendarUpdateEvent {
    pub(crate) fn new(runtime: Option<GoogleCalendarRuntimeConfig>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl Tool for GoogleCalendarUpdateEvent {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let calendar_id =
            required_string(&arguments, "calendar_id", "google_calendar_update_event")?;
        let event_id = required_string(&arguments, "event_id", "google_calendar_update_event")?;
        let mut patch = Map::new();
        add_optional_text(
            &mut patch,
            &arguments,
            "summary",
            1024,
            "google_calendar_update_event",
        )?;
        add_optional_text(
            &mut patch,
            &arguments,
            "description",
            8192,
            "google_calendar_update_event",
        )?;
        add_optional_text(
            &mut patch,
            &arguments,
            "location",
            1024,
            "google_calendar_update_event",
        )?;
        let start = arguments.get("start").and_then(|value| value.as_str());
        let end = arguments.get("end").and_then(|value| value.as_str());
        if start.is_some() != end.is_some() {
            return invalid(
                "google_calendar_update_event",
                "provide both 'start' and 'end' when changing event time",
            );
        }
        if let (Some(start), Some(end)) = (start, end) {
            if parse_datetime(start, "start", "google_calendar_update_event")?
                >= parse_datetime(end, "end", "google_calendar_update_event")?
            {
                return invalid(
                    "google_calendar_update_event",
                    "'start' must be earlier than 'end'",
                );
            }
            patch.insert("start".into(), json!({"dateTime": start}));
            patch.insert("end".into(), json!({"dateTime": end}));
        }
        if patch.is_empty() {
            return invalid(
                "google_calendar_update_event",
                "provide at least one event field to update",
            );
        }

        let (client, authorized, public_id) =
            authorized_client(self.runtime.as_ref(), context, calendar_id).await?;
        let updated = client
            .update_event(
                &authorized.calendar.provider_calendar_id,
                event_id,
                Value::Object(patch),
            )
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        tracing::info!(
            operation = "update",
            user_id = context.user_id.as_deref().unwrap_or("unknown"),
            calendar_id = %public_id,
            event_id = updated.get("id").and_then(|value| value.as_str()).unwrap_or(event_id),
            "Google Calendar event write completed"
        );
        serde_json::to_string_pretty(
            &json!({"calendar_id": public_id, "event": public_event(updated)}),
        )
        .map_err(|error| ToolError::Execution(error.to_string()))
    }
}

async fn authorized_client(
    runtime: Option<&GoogleCalendarRuntimeConfig>,
    context: &ToolContext,
    calendar_id: &str,
) -> Result<
    (
        GoogleCalendarClient,
        agent_service::AuthorizedGoogleCalendar,
        String,
    ),
    ToolError,
> {
    let runtime = runtime.ok_or_else(|| {
        ToolError::Configuration("Google Calendar OAuth is not configured on this server.".into())
    })?;
    let user_id = context.user_id.as_deref().ok_or_else(|| {
        ToolError::Configuration("Google Calendar requires an authenticated EME user.".into())
    })?;
    let family_id = context
        .family
        .as_ref()
        .map(|family| family.family_id.as_str());
    // This rechecks owner/family permissions and Google write scope before token decryption.
    let authorized = runtime
        .access
        .authorize_write(user_id, calendar_id, family_id)
        .await
        .map_err(|error| ToolError::Execution(error.to_string()))?;
    let token = runtime
        .refresh_access_token(authorized.calendar.connection_id, &authorized.refresh_token)
        .await
        .map_err(|error| ToolError::Execution(error.to_string()))?;
    let client = GoogleCalendarClient::from_access_token(token.access_token);
    let public_id = authorized.calendar.id.to_string();
    Ok((client, authorized, public_id))
}

fn required_string<'a>(args: &'a Value, name: &str, tool: &str) -> Result<&'a str, ToolError> {
    args.get(name)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ToolError::InvalidArguments {
            tool_name: tool.into(),
            errors: vec![format!("'{name}' must be a non-empty string.")],
        })
}
fn required_datetime<'a>(args: &'a Value, name: &str, tool: &str) -> Result<&'a str, ToolError> {
    let value = required_string(args, name, tool)?;
    parse_datetime(value, name, tool)?;
    Ok(value)
}
fn parse_datetime(
    value: &str,
    name: &str,
    tool: &str,
) -> Result<DateTime<chrono::FixedOffset>, ToolError> {
    DateTime::parse_from_rfc3339(value).map_err(|_| ToolError::InvalidArguments {
        tool_name: tool.into(),
        errors: vec![format!(
            "'{name}' must be an RFC 3339 timestamp with an explicit timezone."
        )],
    })
}
fn add_optional_text(
    target: &mut Map<String, Value>,
    args: &Value,
    name: &str,
    max_len: usize,
    tool: &str,
) -> Result<(), ToolError> {
    if let Some(value) = args.get(name) {
        let value = value.as_str().ok_or_else(|| ToolError::InvalidArguments {
            tool_name: tool.into(),
            errors: vec![format!("'{name}' must be a string.")],
        })?;
        if value.chars().count() > max_len {
            return invalid(tool, &format!("'{name}' exceeds {max_len} characters"));
        }
        target.insert(name.into(), Value::String(value.to_owned()));
    }
    Ok(())
}
fn invalid<T>(tool: &str, message: &str) -> Result<T, ToolError> {
    Err(ToolError::InvalidArguments {
        tool_name: tool.into(),
        errors: vec![message.into()],
    })
}
fn public_event(event: Value) -> Value {
    let mut output = Map::new();
    for field in [
        "id",
        "summary",
        "description",
        "location",
        "start",
        "end",
        "status",
        "updated",
        "htmlLink",
    ] {
        if let Some(value) = event.get(field) {
            output.insert(field.into(), value.clone());
        }
    }
    Value::Object(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_timezone_free_datetimes() {
        assert!(parse_datetime("2026-11-18T10:00:00", "start", "tool").is_err());
        assert!(parse_datetime("2026-11-18T10:00:00+01:00", "start", "tool").is_ok());
    }
    #[test]
    fn event_response_exposes_only_safe_fields() {
        let event = public_event(
            json!({"id":"event-1","summary":"Planning","organizer":{"email":"private@example.com"}}),
        );
        assert_eq!(event["id"], "event-1");
        assert!(event.get("organizer").is_none());
    }
}
