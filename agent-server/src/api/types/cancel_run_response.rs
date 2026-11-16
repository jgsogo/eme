//! Response describing the outcome of a run cancellation request.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CancelRunResponse {
    pub run_id: String,
    pub status: &'static str,
}
