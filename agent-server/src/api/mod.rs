//! HTTP route modules and API types shared with the server binary.

mod agents;
mod conversations;
#[rustfmt::skip]
mod books;
mod families;
mod google_oauth;
mod meal_planner;
mod media;
mod notes;
mod profile;
mod runs;
mod school_menus;
mod tasks;
mod types;

#[cfg(test)]
mod tests;

pub use agents::agents_api;
pub use books::books_api;
pub use conversations::{
    ContinueConversationRequest, ConversationSummary, CreateConversationRequest,
    UpdateConversationRequest, conversations_api,
};
pub use families::{LanInvitationConfig, families_api, invitations_api};
pub use google_oauth::{GoogleOAuthState, google_integrations_api};
pub use meal_planner::meal_planner_api;
pub use media::{MediaApiState, media_api};
pub use notes::notes_api;
pub use profile::profile_api;
pub use runs::runs_api;
pub use school_menus::school_menus_api;
pub use tasks::tasks_api;
pub use types::{
    AgentCatalog, AgentListItem, CancelRunResponse, GetRunResponse, RunEventResponse,
    SubmitRunRequest, SubmitRunResponse,
};
