use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct UserMe {
    pub(super) id: String,
}
