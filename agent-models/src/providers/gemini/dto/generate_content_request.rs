use super::*;
use serde::{Serialize};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::providers::gemini) struct GenerateContentRequest {
    pub(in crate::providers::gemini) contents: Vec<Content>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::providers::gemini) system_instruction: Option<SystemInstruction>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::providers::gemini) generation_config: Option<GenerationConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::providers::gemini) tools: Option<Vec<GeminiTool>>,
}
