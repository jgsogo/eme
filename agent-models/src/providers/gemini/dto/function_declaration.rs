use serde::Serialize;
#[derive(Serialize)]
pub(in crate::providers::gemini) struct FunctionDeclaration {
    pub(in crate::providers::gemini) name: String,
    pub(in crate::providers::gemini) description: String,
    pub(in crate::providers::gemini) parameters: serde_json::Value,
}
