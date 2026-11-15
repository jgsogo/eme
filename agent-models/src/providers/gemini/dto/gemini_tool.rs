use super::*;
use serde::{Serialize};
#[derive(Serialize)]
pub(super) struct GeminiTool {
    pub(super) function_declarations: Vec<FunctionDeclaration>,
}
