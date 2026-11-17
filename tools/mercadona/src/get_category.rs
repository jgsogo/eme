//! Agent tool for retrieving a Mercadona category.

use crate::{
    arguments::{required_string, warehouse},
    client::MercadonaClient,
};
use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;

/// Retrieve a category and its products/subcategories.
pub struct GetCategory;

#[async_trait]
impl Tool for GetCategory {
    async fn execute(&self, _context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let id = required_string(&arguments, "category_id")?;
        let client = MercadonaClient::new(warehouse(&arguments)).map_err(ToolError::Execution)?;
        let result = client.category(id).await.map_err(ToolError::Execution)?;
        serde_json::to_string_pretty(&result)
            .map_err(|error| ToolError::Execution(error.to_string()))
    }
}
