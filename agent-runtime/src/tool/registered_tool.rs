use std::sync::Arc;
use crate::ToolDefinition;
use super::Tool;

/// In-memory registry of executable tools.
#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, RegisteredTool>,
}

pub(super) struct RegisteredTool {
    pub(super) definition: ToolDefinition,
    pub(super) implementation: Arc<dyn Tool>,
    pub(super) validator: jsonschema::Validator,
}

