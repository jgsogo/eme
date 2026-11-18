//! Trusted host-provided metadata passed to every tool invocation.

use uuid::Uuid;

use super::FamilyContext;

/// Trusted execution metadata supplied by the platform, never by the model.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolContext {
    /// Persistent EME user identifier resolved from the authenticated platform identity.
    pub user_id: Option<String>,
    /// Host-resolved Telegram user ID linked to the authenticated conversation owner; never model-controlled.
    pub telegram_id: Option<String>,
    /// Family selected for the conversation and verified for this user.
    pub family: Option<FamilyContext>,
    /// Conversation whose turn caused this tool invocation.
    pub conversation_id: Option<String>,
    /// Stable identifier of the enclosing agent run.
    pub run_id: Option<Uuid>,
}

impl ToolContext {
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            user_id: Some(user_id.into()),
            ..Self::default()
        }
    }
}
