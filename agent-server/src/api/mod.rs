//! HTTP route modules and API types shared with the server binary.

mod agents;
mod conversations;
mod families;
mod runs;
mod types;

#[cfg(test)]
mod tests;

pub use agents::agents_api;
pub use conversations::{
    ContinueConversationRequest, ConversationSummary, CreateConversationRequest,
    UpdateConversationRequest, conversations_api,
};
pub use families::{LanInvitationConfig, families_api, invitations_api};
pub use runs::runs_api;
pub use types::{
    AgentCatalog, AgentListItem, CancelRunResponse, GetRunResponse, RunEventResponse,
    SubmitRunRequest, SubmitRunResponse,
};
