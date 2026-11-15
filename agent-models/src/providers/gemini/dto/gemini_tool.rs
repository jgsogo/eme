use super::*;
use serde::{Serialize};
#[derive(Serialize)]
pub(in crate::providers::gemini) struct GeminiTool {
    pub(in crate::providers::gemini) function_declarations: Vec<FunctionDeclaration>,
}
