use serde::{Deserialize};
#[derive(Deserialize)]
pub(super) struct ApiUsage {
    pub(super) prompt_tokens: u32,
    pub(super) completion_tokens: u32,
}
