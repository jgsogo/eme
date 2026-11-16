use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct GetRunResponse {
    pub run_id: String,
    pub agent: String,
    pub status: &'static str,
    pub response: Option<String>,
    pub error: Option<String>,
}
