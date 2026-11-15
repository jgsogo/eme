use serde::{Deserialize};
#[derive(Deserialize)]
pub(super) struct ApiResponse {
    pub(super) choices: Vec<Choice>,
    pub(super) usage: Option<ApiUsage>,
}
