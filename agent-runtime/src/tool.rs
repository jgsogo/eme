mod dispatcher;
mod error;
mod registered_tool;
mod registry;
mod tool;

use registered_tool::RegisteredTool;
use uuid::Uuid;

/// Trusted family membership information resolved by the platform before a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyContext {
    pub family_id: String,
    pub family_name: String,
    pub family_role: String,
    pub relationship: String,
}


/// Trusted execution metadata supplied by the platform, never by the model.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolContext {
    /// Persistent EME user identifier resolved from the authenticated platform identity.
    pub user_id: Option<String>,
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

pub use dispatcher::ToolDispatcher;
pub use error::ToolError;
pub use registry::ToolRegistry;
pub use tool::Tool;
