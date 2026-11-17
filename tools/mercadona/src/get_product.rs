//! Agent tool for retrieving full Mercadona product details.

use crate::{
    arguments::{required_string, warehouse},
    client::MercadonaClient,
};
use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;

/// Retrieve the full details of one product.
pub struct GetProduct;

#[async_trait]
impl Tool for GetProduct {
    async fn execute(&self, _context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let id = required_string(&arguments, "product_id")?;
        let client = MercadonaClient::new(warehouse(&arguments)).map_err(ToolError::Execution)?;
        let result = client.product(id).await.map_err(ToolError::Execution)?;
        serde_json::to_string_pretty(&result)
            .map_err(|error| ToolError::Execution(error.to_string()))
    }
}
