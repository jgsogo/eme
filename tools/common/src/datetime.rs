use agent_runtime::{Tool, ToolError};
use async_trait::async_trait;
use chrono::{SecondsFormat, Utc};
use serde_json::{Value, json};

/// Returns the current UTC date and time, including its timezone offset.
pub struct CurrentDateTime;

#[async_trait]
impl Tool for CurrentDateTime {
    async fn execute(&self, _arguments: Value) -> Result<String, ToolError> {
        let now = Utc::now();

        serde_json::to_string_pretty(&json!({
            "datetime": now.to_rfc3339_opts(SecondsFormat::Millis, true),
            "timezone": "UTC",
            "unix_timestamp": now.timestamp(),
            "unix_timestamp_millis": now.timestamp_millis(),
        }))
        .map_err(|error| ToolError::Execution(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_a_full_utc_timestamp_and_unix_time() {
        let result = CurrentDateTime.execute(Value::Null).await.unwrap();
        let value: Value = serde_json::from_str(&result).unwrap();

        let datetime = value["datetime"].as_str().unwrap();
        assert!(datetime.ends_with('Z'));
        assert_eq!(datetime.len(), 24);
        assert_eq!(value["timezone"], "UTC");
        assert!(value["unix_timestamp"].as_i64().is_some());
        assert!(value["unix_timestamp_millis"].as_i64().is_some());
    }
}
