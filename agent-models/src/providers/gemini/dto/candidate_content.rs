use super::*;
use serde::Deserialize;
#[derive(Deserialize)]
pub(in crate::providers::gemini) struct CandidateContent {
    pub(in crate::providers::gemini) parts: Vec<PartResponse>,
}
