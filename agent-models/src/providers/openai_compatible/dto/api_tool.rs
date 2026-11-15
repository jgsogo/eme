use super::*;
use serde::Serialize;
#[derive(Serialize)]
pub(in crate::providers::openai_compatible) struct ApiTool {
    #[serde(rename = "type")]
    pub(in crate::providers::openai_compatible) kind: String,
    pub(in crate::providers::openai_compatible) function: ApiFunctionDefinition,
}
