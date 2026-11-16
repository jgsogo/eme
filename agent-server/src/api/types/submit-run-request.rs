use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SubmitRunRequest {
    pub agent: String,
    pub input: String,
}
