//! Policy-engine contract for deciding whether a model-requested tool may run.

use super::Authorization;
use crate::{Agent, ToolCall};

/// Determines whether an agent is allowed to execute a tool call.
pub trait PolicyEngine: Send + Sync {
    fn authorize(&self, agent: &Agent, call: &ToolCall) -> Authorization;
}
