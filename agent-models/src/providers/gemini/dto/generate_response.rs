use super::*;
use serde::{Deserialize};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GenerateResponse {
    pub(super) candidates: Option<Vec<Candidate>>,
    pub(super) usage_metadata: Option<UsageMetadata>,
}
