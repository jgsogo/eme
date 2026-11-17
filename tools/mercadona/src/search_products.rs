//! Agent tool for searching Mercadona products through Algolia.

use crate::{arguments::warehouse, client::MercadonaClient};
use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;

/// Search the Mercadona product catalogue.
pub struct SearchProducts;

#[async_trait]
impl Tool for SearchProducts {
    async fn execute(&self, _context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let query = arguments
            .get("query")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| ToolError::Execution("'query' must be a non-empty string".into()))?;
        let limit = arguments.get("limit").and_then(Value::as_u64).unwrap_or(10) as usize;
        if !(1..=50).contains(&limit) {
            return Err(ToolError::Execution(
                "'limit' must be between 1 and 50".into(),
            ));
        }

        let client = MercadonaClient::new(warehouse(&arguments)).map_err(ToolError::Execution)?;
        let result = client
            .search_products(query, limit)
            .await
            .map_err(ToolError::Execution)?;
        serde_json::to_string_pretty(&result)
            .map_err(|error| ToolError::Execution(error.to_string()))
    }
}
