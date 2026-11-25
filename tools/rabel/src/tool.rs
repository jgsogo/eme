//! Agent tool adapters for compact RABEL catalogue queries.

use std::sync::Arc;

use agent_runtime::{Tool, ToolContext, ToolError, ToolDefinition};
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::RabelClient;

pub struct SearchCatalog(pub(super) Arc<RabelClient>);
pub struct GetItem(pub(super) Arc<RabelClient>);
pub struct ListLibraries(pub(super) Arc<RabelClient>);

pub fn search_definition() -> ToolDefinition {
    ToolDefinition {
        name: "rabel.search_catalog".into(),
        description: "Search the Salamanca Public Library catalogue. Returns compact bibliographic matches; call rabel.get_item only for the selected record.".into(),
        parameters: json!({
            "type":"object",
            "properties":{
                "query":{"type":"string","minLength":1,"description":"Title, author, ISBN or general catalogue search terms."},
                "limit":{"type":"integer","minimum":1,"maximum":20,"default":10}
            },
            "required":["query"],
            "additionalProperties":false
        }),
    }
}

pub fn item_definition() -> ToolDefinition {
    ToolDefinition {
        name: "rabel.get_item".into(),
        description: "Retrieve bibliographic metadata for one record returned by rabel.search_catalog. This describes the resource itself; it does not include library copies or availability.".into(),
        parameters: json!({
            "type":"object",
            "properties":{"record_ref":{"type":"string","minLength":1,"description":"Opaque record_ref returned by rabel.search_catalog."}},
            "required":["record_ref"],
            "additionalProperties":false
        }),
    }
}

pub fn libraries_definition() -> ToolDefinition {
    ToolDefinition {
        name: "rabel.list_libraries".into(),
        description: "List the libraries exposed by the RABEL catalogue selector.".into(),
        parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
    }
}

fn execution(error: impl std::fmt::Display) -> ToolError {
    ToolError::Execution(error.to_string())
}

#[async_trait]
impl Tool for SearchCatalog {
    async fn execute(&self, _context: &ToolContext, args: Value) -> Result<String, ToolError> {
        let query = args.get("query").and_then(Value::as_str).filter(|q| !q.trim().is_empty())
            .ok_or_else(|| execution("query must be a non-empty string"))?;
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(10) as usize;
        let results = self.0.search(query, limit).await.map_err(execution)?;
        serde_json::to_string(&results).map_err(execution)
    }
}

#[async_trait]
impl Tool for GetItem {
    async fn execute(&self, _context: &ToolContext, args: Value) -> Result<String, ToolError> {
        let record_ref = args.get("record_ref").and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| execution("record_ref must be a non-empty string"))?;
        let item = self.0.get_item(record_ref).await.map_err(execution)?;
        serde_json::to_string(&item).map_err(execution)
    }
}

#[async_trait]
impl Tool for ListLibraries {
    async fn execute(&self, _context: &ToolContext, _args: Value) -> Result<String, ToolError> {
        let libraries = self.0.libraries().await.map_err(execution)?;
        serde_json::to_string(&libraries).map_err(execution)
    }
}
