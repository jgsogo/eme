//! Trusted host-provided metadata passed to every tool invocation.

use uuid::Uuid;

use super::FamilyContext;

/// Trusted execution metadata supplied by the platform, never by the model.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolContext {
    /// Persistent EME user identifier resolved from the authenticated platform identity.
    pub user_id: Option<String>,
    /// IANA timezone configured in the authenticated user's platform profile.
    pub timezone: Option<String>,
    /// Host-resolved Telegram user ID linked to the authenticated conversation owner; never model-controlled.
    pub telegram_id: Option<String>,
    /// Family selected for this run and verified for this user.
    pub family: Option<FamilyContext>,
    /// Family rosters the host authorizes this run to use. The host may scope this
    /// to a selected family even when the user belongs to additional families.
    /// Telegram recipient authorization is checked against these trusted rosters.
    pub families: Vec<FamilyContext>,
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
