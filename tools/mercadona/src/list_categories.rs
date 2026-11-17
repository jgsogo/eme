//! Agent tool for listing Mercadona product categories.

use crate::{arguments::warehouse, client::MercadonaClient};
use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;

/// Retrieve the category tree for a warehouse.
pub struct ListCategories;

#[async_trait]
impl Tool for ListCategories {
    async fn execute(&self, _context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let client = MercadonaClient::new(warehouse(&arguments)).map_err(ToolError::Execution)?;
        let result = client.categories().await.map_err(ToolError::Execution)?;
        serde_json::to_string_pretty(&result)
            .map_err(|error| ToolError::Execution(error.to_string()))
    }
}
