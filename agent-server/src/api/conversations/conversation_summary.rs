//! Compact conversation metadata returned by listing and update routes.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ConversationSummary {
    pub id: String,
    pub agent: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub selected_family_id: Option<String>,
    pub history_tokens_estimate: usize,
}
