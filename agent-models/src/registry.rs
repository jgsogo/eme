use crate::{
    client::ConversationClient,
    error::ModelError,
    models::{ModelCapability, ModelConfig},
    providers::{GeminiClient, OpenAICompatibleClient},
};
use agent_config::{ConfigError, read_toml_dir};
use serde::Deserialize;
use std::sync::Arc;
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Deserialize)]
struct ProviderConfig {
    name: String,
    client: String,
    api_key_env: String,
    account_id_env: Option<String>,
    base_url: Option<String>,
    models: Vec<ModelConfig>,
}

impl ProviderConfig {
    fn validate(&self, path: &Path) -> Result<(), ConfigError> {
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

pub struct Registry {
    providers: BTreeMap<String, ProviderConfig>,
}

impl Registry {
    pub fn load(data_dir: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let providers_dir = data_dir.as_ref().join("providers");
        let loaded = read_toml_dir::<ProviderConfig>(&providers_dir)?;

        if loaded.is_empty() {
            return Err(ConfigError::no_files("provider", providers_dir));
        }

        let mut providers = BTreeMap::new();

        for (path, config) in loaded {
            config.validate(&path)?;
            let name = config.name.clone();

            if providers.insert(name.clone(), config).is_some() {
                return Err(ConfigError::duplicate("provider", name, Some(path)));
            }
        }

        Ok(Self { providers })
    }

    /// Returns whether a provider/model pair is declared in the loaded configuration.
    ///
    /// This check does not read credentials or construct a provider client.
    pub fn contains_model(&self, model_name: &str) -> bool {
        let Some((provider_name, model_name)) = model_name.split_once('/') else {
            return false;
        };

        if provider_name.is_empty() || model_name.is_empty() {
            return false;
        }

        self.providers
            .get(provider_name)
            .is_some_and(|provider| provider.models.iter().any(|model| model.name == model_name))
    }

    pub fn models_with_capability(
        &self,
        capability: ModelCapability,
    ) -> Vec<(String, ModelConfig)> {
        let mut result = Vec::<(String, ModelConfig)>::new();
        for (provider_name, provider_cfg) in self.providers.iter() {
            for model in provider_cfg.models.iter() {
                if model.capabilities.iter().any(|cap| cap == &capability) {
                    result.push((provider_name.clone(), model.clone()));
                }
            }
        }

        result
    }

    pub fn resolve_model(
        &self,
        model_name: &str,
    ) -> Result<Arc<dyn ConversationClient>, ModelError> {
        let (provider_name, model) = model_name
            .split_once('/')
            .ok_or_else(|| ModelError::InvalidModelName(model_name.to_string()))?;

        let provider = self
            .providers
            .get(provider_name)
            .ok_or_else(|| ModelError::UnsupportedProvider(provider_name.to_string()))?;

        if !provider.models.iter().any(|m| m.name == model) {
            return Err(ModelError::UnsupportedModel(format!(
                "{provider_name}/{model}"
            )));
        }

        let key = std::env::var(&provider.api_key_env)
            .map_err(|_| ModelError::MissingApiKey(provider.api_key_env.clone()))?;

        match provider.client.as_str() {
            "gemini" => Ok(Arc::new(GeminiClient::new(key, model))),
            "openai-compatible" => {
                let base_url = match (&provider.account_id_env, &provider.base_url) {
                    (Some(account_id_env), Some(url)) => {
                        let account_id = std::env::var(account_id_env)
                            .map_err(|_| ModelError::MissingApiKey(account_id_env.clone()))?;
                        url.replace("{account_id}", &account_id)
                    }
                    (None, Some(url)) => url.clone(),
                    _ => return Err(ModelError::UnsupportedProvider(provider.name.clone())),
                };

                Ok(Arc::new(OpenAICompatibleClient::new(&base_url, key, model)))
            }
            _ => Err(ModelError::UnsupportedProvider(provider.client.clone())),
        }
    }
}
