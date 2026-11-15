use super::*;
use serde::Deserialize;
#[derive(Deserialize)]
pub(in crate::providers::openai_compatible) struct Choice {
    pub(in crate::providers::openai_compatible) message: ApiMessageResponse,
}
