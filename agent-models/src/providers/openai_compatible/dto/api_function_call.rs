use serde::{Serialize};
#[derive(Serialize)]
pub(super) struct ApiFunctionCall {
    pub(super) name: String,
    pub(super) arguments: String,
}
