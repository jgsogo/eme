use serde::Serialize;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::providers::gemini) struct GenerationConfig {
    pub(in crate::providers::gemini) temperature: Option<f32>,
    pub(in crate::providers::gemini) max_output_tokens: Option<u32>,
}
