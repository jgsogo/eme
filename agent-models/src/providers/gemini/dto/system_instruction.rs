use super::*;
use serde::{Serialize};
#[derive(Serialize)]
pub(in crate::providers::gemini) struct SystemInstruction {
    pub(in crate::providers::gemini) parts: Vec<Part>,
}
