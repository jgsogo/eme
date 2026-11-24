//! HTTP interface for the EME agent platform.
//!
//! This crate exposes endpoints for agent discovery, conversation management,
//! run submission and lifecycle inspection, and family membership operations.
//!
//! Authentication is provided through Telegram-backed browser sessions. The
//! executable composition, background workers, and external adapters live in
//! the `agent-server` binary rather than in this API library.

mod api;
pub mod auth;

pub use api::{
    AgentCatalog, AgentListItem, CancelRunResponse, ContinueConversationRequest,
    ConversationSummary, CreateConversationRequest, GetRunResponse, GoogleOAuthState,
    LanInvitationConfig, MediaApiState, RunEventResponse, SubmitRunRequest, SubmitRunResponse,
    UpdateConversationRequest, agents_api, books_api, conversations_api, families_api,
    google_integrations_api, invitations_api, libraries_api, meal_planner_api, media_api, notes_api, profile_api,
    runs_api, school_menus_api, tasks_api,
};
