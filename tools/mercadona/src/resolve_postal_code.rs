//! Agent tool for resolving a Spanish postal code to a Mercadona warehouse.

use crate::{arguments::required_string, client::MercadonaClient};
use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;

/// Resolve the warehouse serving a Spanish postal code.
pub struct ResolvePostalCode;

#[async_trait]
impl Tool for ResolvePostalCode {
    async fn execute(&self, _context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let postal_code = required_string(&arguments, "postal_code")?;
        if postal_code.len() != 5 || !postal_code.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(ToolError::Execution(
                "'postal_code' must contain exactly five digits".into(),
            ));
        }
        let client = MercadonaClient::new("mad1").map_err(ToolError::Execution)?;
        let result = client
            .resolve_postal_code(postal_code)
            .await
            .map_err(ToolError::Execution)?;
        serde_json::to_string_pretty(&result)
            .map_err(|error| ToolError::Execution(error.to_string()))
    }
}
