use serde::{Deserialize};
#[derive(Deserialize)]
pub(super) struct CandidateContent {
    pub(super) parts: Vec<PartResponse>,
}
