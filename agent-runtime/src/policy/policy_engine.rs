use crate::{Agent, ToolCall};
use super::Authorization;

/// Determines whether an agent is allowed to execute a tool call.
pub trait PolicyEngine: Send + Sync {
    fn authorize(&self, agent: &Agent, call: &ToolCall) -> Authorization;
}
