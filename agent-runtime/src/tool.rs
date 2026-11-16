mod dispatcher;
mod error;
mod registered_tool;
mod registry;
mod tool;

use registered_tool::RegisteredTool;
use uuid::Uuid;

/// Trusted family membership information resolved by the platform before a run.\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct FamilyContext {\n    pub family_id: String,\n    pub family_name: String,\n    pub family_role: String,\n    pub relationship: String,\n}\n

/// Trusted execution metadata supplied by the platform, never by the model.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolContext {
    /// Authenticated platform user identifier (for example `telegram:123`).
    pub user_id: Option<String>,
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
