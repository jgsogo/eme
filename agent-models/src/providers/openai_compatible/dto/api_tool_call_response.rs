use super::*;
use serde::{Deserialize};
#[derive(Deserialize)]
pub(super) struct ApiToolCallResponse {
    pub(super) id: String,
    pub(super) function: ApiFunctionCallResponse,
}
