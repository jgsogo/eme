use super::*;
use serde::Serialize;
#[derive(Serialize)]
pub(in crate::providers::gemini) struct Content {
    pub(in crate::providers::gemini) role: String,
    pub(in crate::providers::gemini) parts: Vec<Part>,
}
