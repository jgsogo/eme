use super::*;
use serde::{Serialize};
#[derive(Serialize)]
pub(super) struct ApiFunctionDefinition {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) parameters: serde_json::Value,
}
