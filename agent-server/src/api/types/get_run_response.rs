//! Public representation of a run's current status and result.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct GetRunResponse {
    pub run_id: String,
    pub conversation_id: Option<String>,
    pub input: String,
    pub agent: String,
    pub status: &'static str,
    pub response: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub one_shot: bool,
}
