//! Bounded asynchronous delay tool.

use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::time::Duration;

const MAX: u64 = 30;

pub struct Sleep;

#[async_trait]
impl Tool for Sleep {
    async fn execute(&self, _context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let seconds = arguments
            .get("seconds")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                ToolError::Execution("'seconds' must be a non-negative integer".into())
            })?;

        if seconds > MAX {
            return Err(ToolError::Execution(format!(
                "sleep cannot exceed {MAX} seconds"
            )));
        }

        tokio::time::sleep(Duration::from_secs(seconds)).await;
        serde_json::to_string_pretty(&json!({
            "slept": true,
            "duration_seconds": seconds
        }))
        .map_err(|error| ToolError::Execution(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn zero_sleep() {
        let value: Value = serde_json::from_str(
            &Sleep
                .execute(&ToolContext::default(), json!({"seconds": 0}))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(value["slept"], true);
    }

    #[tokio::test]
    async fn max_is_enforced() {
        assert!(
            Sleep
                .execute(&ToolContext::default(), json!({"seconds": 31}))
                .await
                .is_err()
        );
    }
}
