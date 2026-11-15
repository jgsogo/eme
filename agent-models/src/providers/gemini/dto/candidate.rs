use super::*;
use serde::Deserialize;
#[derive(Deserialize)]
pub(in crate::providers::gemini) struct Candidate {
    pub(in crate::providers::gemini) content: CandidateContent,
}
