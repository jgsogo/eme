//! Internal registered tool binding with its compiled argument schema validator.

use std::sync::Arc;

use super::Tool;
use crate::ToolDefinition;

/// Registered implementation and its compiled argument validator.
pub(super) struct RegisteredTool {
    pub(super) definition: ToolDefinition,
    pub(super) implementation: Arc<dyn Tool>,
    pub(super) validator: jsonschema::Validator,
}
