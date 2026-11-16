//! Typed errors returned by tool lookup, validation, and execution.

use crate::ErrorCategory;

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Unknown tool: {0}")]
    UnknownTool(String),

    #[error("Invalid arguments for tool '{tool_name}': {errors:?}")]
    InvalidArguments {
        tool_name: String,
        errors: Vec<String>,
    },

    #[error("Tool configuration failed: {0}")]
    Configuration(String),

    #[error("Tool execution failed: {0}")]
    Execution(String),
}

impl ToolError {
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::Configuration(_) => ErrorCategory::Configuration,
            Self::UnknownTool(_) | Self::InvalidArguments { .. } | Self::Execution(_) => {
                ErrorCategory::Tool
            }
        }
    }
}
