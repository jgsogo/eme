/// Error explaining why a tool invocation was denied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Authorization denied for tool '{tool_name}': {reason}")]
pub struct AuthorizationError {
    pub tool_name: String,
    pub reason: String,
}
