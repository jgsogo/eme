use super::*;
use serde::{Deserialize};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UsageMetadata {
    pub(super) prompt_token_count: u32,
    pub(super) candidates_token_count: u32,
}
