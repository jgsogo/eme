//! Serialized lifecycle event exposed by the run events endpoint.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct RunEventResponse {
    pub sequence: u64,
    pub kind: &'static str,
    pub iteration: Option<usize>,
    pub tool_name: Option<String>,
    pub error_category: Option<&'static str>,
    pub occurred_at_unix_ms: u128,
}
