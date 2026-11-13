use crate::models::ModelCapability;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ModelConfig {
    pub name: String,
    pub capabilities: Vec<ModelCapability>,
}
