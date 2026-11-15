use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct UserMe {
    pub(crate) id: String,
}
