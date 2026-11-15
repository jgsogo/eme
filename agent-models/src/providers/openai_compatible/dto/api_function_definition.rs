use super::*;
use serde::{Serialize};
#[derive(Serialize)]
pub(in crate::providers::openai_compatible) struct ApiFunctionDefinition {
    pub(in crate::providers::openai_compatible) name: String,
    pub(in crate::providers::openai_compatible) description: String,
    pub(in crate::providers::openai_compatible) parameters: serde_json::Value,
}
