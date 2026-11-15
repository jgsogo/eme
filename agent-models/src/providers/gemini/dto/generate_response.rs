use super::*;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::providers::gemini) struct GenerateResponse {
    pub(in crate::providers::gemini) candidates: Option<Vec<Candidate>>,
    pub(in crate::providers::gemini) usage_metadata: Option<UsageMetadata>,
}
