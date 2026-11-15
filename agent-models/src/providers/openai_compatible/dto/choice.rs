use serde::{Deserialize};
#[derive(Deserialize)]
pub(super) struct Choice {
    pub(super) message: ApiMessageResponse,
}
