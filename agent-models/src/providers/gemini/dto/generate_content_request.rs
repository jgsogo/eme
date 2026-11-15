use serde::{Serialize};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GenerateContentRequest {
    pub(super) contents: Vec<Content>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) system_instruction: Option<SystemInstruction>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) generation_config: Option<GenerationConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tools: Option<Vec<GeminiTool>>,
}
