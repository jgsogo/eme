use crate::models::ModelConfig;
use agent_config::ConfigError;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub(super) struct ProviderConfig {
    pub(super) name: String,
    pub(super) client: String,
    #[serde(default = "default_enabled")]
    pub(super) enabled: bool,
    pub(super) api_key_env: String,
    pub(super) account_id_env: Option<String>,
    pub(super) base_url: Option<String>,
    pub(super) models: Vec<ModelConfig>,
}

fn default_enabled() -> bool {
    true
}

impl ProviderConfig {
    pub(super) fn validate(&self, path: &Path) -> Result<(), ConfigError> {
        let invalid = |reason: &str| {
            ConfigError::invalid("provider", Some(path), format!("{}: {reason}", self.name))
        };

        if self.name.trim().is_empty() {
            return Err(invalid("provider name cannot be empty"));
        }

        if self.api_key_env.trim().is_empty() {
            return Err(invalid("api_key_env cannot be empty"));
        }

        if self.models.is_empty() {
            return Err(invalid("provider must define at least one model"));
        }

        let mut model_names = std::collections::HashSet::new();

        for model in &self.models {
            model.validate().map_err(|reason| invalid(&reason))?;

            if !model_names.insert(&model.name) {
                return Err(invalid(&format!(
                    "model '{}' is declared more than once",
                    model.name
                )));
            }
        }

        match self.client.as_str() {
            "gemini" => Ok(()),
            "openai-compatible" => {
                let base_url = self
                    .base_url
                    .as_deref()
                    .filter(|url| !url.trim().is_empty())
                    .ok_or_else(|| invalid("openai-compatible providers require base_url"))?;

                let uses_account_id = base_url.contains("{account_id}");
                let has_account_id_env = self
                    .account_id_env
                    .as_deref()
                    .is_some_and(|name| !name.trim().is_empty());

                if uses_account_id != has_account_id_env {
                    return Err(invalid(
                        "base_url must use {account_id} if and only if account_id_env is configured",
                    ));
                }

                Ok(())
            }
            client => Err(invalid(&format!("unsupported client '{client}'"))),
        }
    }
}
