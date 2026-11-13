use std::collections::HashSet;

use crate::{Agent, ToolCall};

/// Result of an authorization decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Authorization {
    Allowed,
    Denied(String),
}

/// Determines whether an agent is allowed to execute a tool call.
pub trait PolicyEngine: Send + Sync {
    fn authorize(&self, agent: &Agent, call: &ToolCall) -> Authorization;
}

/// Allows execution of explicitly configured tools.
pub struct AllowListPolicy {
    allowed_tools: HashSet<String>,
}

impl AllowListPolicy {
    pub fn new<I, S>(allowed_tools: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            allowed_tools: allowed_tools.into_iter().map(Into::into).collect(),
        }
    }
}

impl PolicyEngine for AllowListPolicy {
    fn authorize(&self, agent: &Agent, call: &ToolCall) -> Authorization {
        if !agent.tool_names.iter().any(|name| name == &call.name) {
            return Authorization::Denied(format!(
                "Tool '{}' is not available to agent '{}'",
                call.name, agent.id
            ));
        }

        if !self.allowed_tools.contains(&call.name) {
            return Authorization::Denied(format!("Tool '{}' is not allowed by policy", call.name));
        }

        Authorization::Allowed
    }
}
