use super::*;
use serde::{Serialize};
#[derive(Serialize)]
pub(super) struct ApiTool {
    #[serde(rename = "type")]
    pub(super) kind: String,
    pub(super) function: ApiFunctionDefinition,
}
