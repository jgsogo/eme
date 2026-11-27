//! Runtime adapters for the individual Home Assistant operations.

use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::{client::HomeAssistantClient, operation::Operation};

/// Executes one fixed Home Assistant operation.
pub struct HomeAssistantTool {
    operation: Operation,
}

impl HomeAssistantTool {
    pub fn new(operation: Operation) -> Self {
        Self { operation }
    }

    fn argument<'a>(
        arguments: &'a Value,
        key: &str,
        tool_name: &str,
    ) -> Result<&'a str, ToolError> {
        arguments
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| ToolError::InvalidArguments {
                tool_name: tool_name.into(),
                errors: vec![format!("Missing or invalid '{key}' argument")],
            })
    }

    fn optional_argument<'a>(arguments: &'a Value, key: &str) -> Option<&'a str> {
        arguments
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
    }

    fn validate_identifier(value: &str, label: &str, tool_name: &str) -> Result<(), ToolError> {
        let valid = !value.is_empty()
            && value != "."
            && value != ".."
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.');
        if valid {
            Ok(())
        } else {
            Err(ToolError::InvalidArguments {
                tool_name: tool_name.into(),
                errors: vec![format!("'{label}' contains unsupported characters")],
            })
        }
    }
}

#[async_trait]
impl Tool for HomeAssistantTool {
    async fn execute(&self, _context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let tool_name = match self.operation {
            Operation::GetStates => "home_assistant.get_states",
            Operation::GetState => "home_assistant.get_state",
            Operation::GetServices => "home_assistant.get_services",
            Operation::CallService => "home_assistant.call_service",
            Operation::GetHistory => "home_assistant.get_history",
            Operation::GetConfig => "home_assistant.get_config",
        };

        let base_url = std::env::var("HOME_ASSISTANT_URL")
            .map_err(|_| ToolError::Configuration("HOME_ASSISTANT_URL env var not set".into()))?;
        let token = std::env::var("HOME_ASSISTANT_TOKEN")
            .map_err(|_| ToolError::Configuration("HOME_ASSISTANT_TOKEN env var not set".into()))?;
        if token.trim().is_empty() {
            return Err(ToolError::Configuration(
                "HOME_ASSISTANT_TOKEN env var is empty".into(),
            ));
        }

        let client = HomeAssistantClient::new(&base_url)
            .map_err(|error| ToolError::Configuration(error.to_string()))?;

        let result = match self.operation {
            Operation::GetStates => client.get_states(&token).await,
            Operation::GetState => {
                let entity_id = Self::argument(&arguments, "entity_id", tool_name)?;
                Self::validate_identifier(entity_id, "entity_id", tool_name)?;
                client.get_state(&token, entity_id).await
            }
            Operation::GetServices => client.get_services(&token).await,
            Operation::CallService => {
                let domain = Self::argument(&arguments, "domain", tool_name)?;
                let service = Self::argument(&arguments, "service", tool_name)?;
                Self::validate_identifier(domain, "domain", tool_name)?;
                Self::validate_identifier(service, "service", tool_name)?;
                let service_data = arguments
                    .get("service_data")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if !service_data.is_object() {
                    return Err(ToolError::InvalidArguments {
                        tool_name: tool_name.into(),
                        errors: vec!["'service_data' must be a JSON object".into()],
                    });
                }
                client
                    .call_service(&token, domain, service, service_data)
                    .await
            }
            Operation::GetHistory => {
                let entity_id = Self::optional_argument(&arguments, "entity_id");
                if let Some(entity_id) = entity_id {
                    Self::validate_identifier(entity_id, "entity_id", tool_name)?;
                }
                let timestamp = Self::optional_argument(&arguments, "timestamp");
                client.get_history(&token, entity_id, timestamp).await
            }
            Operation::GetConfig => client.get_config(&token).await,
        }
        .map_err(|error| ToolError::Execution(error.to_string()))?;

        serde_json::to_string_pretty(&result)
            .map_err(|error| ToolError::Execution(error.to_string()))
    }
}
