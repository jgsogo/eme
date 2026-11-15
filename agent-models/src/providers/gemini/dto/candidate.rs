use super::*;
use serde::{Deserialize};
#[derive(Deserialize)]
pub(super) struct Candidate {
    pub(super) content: CandidateContent,
}
