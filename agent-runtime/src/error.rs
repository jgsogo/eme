use std::time::Duration;

use agent_config::ConfigError;
use agent_models::error::ModelError;
use thiserror::Error;

use crate::{AuthorizationError, ToolError};

/// Stable error classes for runtime decisions and observability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Configuration,
    Authorization,
    Model,
    Tool,
    Timeout,
    Cancellation,
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Configuration(#[from] ConfigError),
    #[error(transparent)]
    Authorization(#[from] AuthorizationError),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error(transparent)]
    Tool(#[from] ToolError),
    #[error("Tool execution timed out after {0:?}")]
    Timeout(Duration),
}

impl RuntimeError {
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::Configuration(_) => ErrorCategory::Configuration,
            Self::Authorization(_) => ErrorCategory::Authorization,
            Self::Model(_) => ErrorCategory::Model,
            Self::Tool(error) => error.category(),
            Self::Timeout(_) => ErrorCategory::Timeout,
        }
    }
}
