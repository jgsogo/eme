use serde::{Serialize};
#[derive(Serialize)]
pub(super) struct Content {
    pub(super) role: String,
    pub(super) parts: Vec<Part>,
}
