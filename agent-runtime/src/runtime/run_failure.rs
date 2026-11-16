//! Failed run outcome, including its error, partial conversation, and emitted events.

use super::RunEvent;
use crate::{ErrorCategory, Message, RuntimeError};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
#[error("Agent run {run_id} failed: {error}")]
pub struct RunFailure {
    pub run_id: Uuid,
    #[source]
    pub error: RuntimeError,
    pub messages: Vec<Message>,
    pub events: Vec<RunEvent>,
    pub iterations: usize,
}

impl RunFailure {
    pub fn category(&self) -> ErrorCategory {
        self.error.category()
    }
}
