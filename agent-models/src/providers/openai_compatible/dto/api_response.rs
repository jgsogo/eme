use super::*;
use serde::{Deserialize};
#[derive(Deserialize)]
pub(in crate::providers::openai_compatible) struct ApiResponse {
    pub(in crate::providers::openai_compatible) choices: Vec<Choice>,
    pub(in crate::providers::openai_compatible) usage: Option<ApiUsage>,
}
