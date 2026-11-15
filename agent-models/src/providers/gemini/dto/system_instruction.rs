use serde::{Serialize};
#[derive(Serialize)]
pub(super) struct SystemInstruction {
    pub(super) parts: Vec<Part>,
}
