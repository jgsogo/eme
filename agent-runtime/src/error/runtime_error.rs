//! Runtime-level error type combining configuration, authorization, model, and tool failures.

use std::time::Duration;

use agent_config::ConfigError;
use agent_models::ModelError;
use thiserror::Error;

use crate::{AuthorizationError, ErrorCategory, ToolError};

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
