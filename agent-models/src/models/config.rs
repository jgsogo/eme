use crate::models::{AccessTier, ModelCapability};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ModelConfig {
    pub name: String,
    pub enabled: bool,
    pub access_tier: AccessTier,
    pub capabilities: Vec<ModelCapability>,
}

impl ModelConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("model name cannot be empty".into());
        }

        let mut capabilities = std::collections::HashSet::new();

        for capability in &self.capabilities {
            if !capabilities.insert(*capability) {
                return Err(format!(
                    "model '{}' declares capability '{capability:?}' more than once",
                    self.name
                ));
            }
        }

        Ok(())
    }
}
