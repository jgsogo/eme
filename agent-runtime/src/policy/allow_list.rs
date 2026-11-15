use std::collections::HashSet;

use super::{Authorization, AuthorizationError, PolicyEngine};
use crate::{Agent, ToolCall};

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
            return Authorization::Denied(AuthorizationError {
                tool_name: call.name.clone(),
                reason: format!("Tool is not available to agent '{}'", agent.id),
            });
        }

        if !self.allowed_tools.contains(&call.name) {
            return Authorization::Denied(AuthorizationError {
                tool_name: call.name.clone(),
                reason: "Tool is not allowed by policy".into(),
            });
        }

        Authorization::Allowed
    }
}
