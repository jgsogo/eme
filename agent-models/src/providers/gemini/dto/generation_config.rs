use super::*;
use serde::{Serialize};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GenerationConfig {
    pub(super) temperature: Option<f32>,
    pub(super) max_output_tokens: Option<u32>,
}
