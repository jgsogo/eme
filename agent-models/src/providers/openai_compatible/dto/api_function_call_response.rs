use serde::{Deserialize};
#[derive(Deserialize)]
pub(super) struct ApiFunctionCallResponse {
    pub(super) name: String,
    pub(super) arguments: String,
}
