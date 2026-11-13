use crate::{
    client::LlmClient,
    error::ModelError,
    providers::{GeminiClient, OpenAICompatibleClient},
};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::{collections::HashMap, sync::Arc};

#[derive(Debug, Deserialize)]
struct ProviderConfig {
    name: String,
    client: String,
    api_key_env: String,
    account_id_env: Option<String>,
    base_url: Option<String>,
    models: Vec<String>,
}

pub struct Registry {
    providers: HashMap<String, ProviderConfig>,
}

impl Registry {
    pub fn load(data_dir: impl AsRef<Path>) -> Result<Self, ModelError> {
        let mut providers = HashMap::new();

        let providers_dir = data_dir.as_ref().join("providers");

        for entry in fs::read_dir(providers_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_none_or(|ext| ext != "toml") {
                continue;
            }

            let contents = fs::read_to_string(path)?;
            let config: ProviderConfig = toml::from_str(&contents)?;

            providers.insert(config.name.clone(), config);
        }

        Ok(Self { providers })
    }

    pub fn resolve_model(&self, model_name: &str) -> Result<Arc<dyn LlmClient>, ModelError> {
        let (provider_name, model) = model_name
            .split_once('/')
            .ok_or_else(|| ModelError::InvalidModelName(model_name.to_string()))?;

        let provider = self
            .providers
            .get(provider_name)
            .ok_or_else(|| ModelError::UnsupportedProvider(provider_name.to_string()))?;

        if !provider.models.iter().any(|m| m == model) {
            return Err(ModelError::UnsupportedProvider(format!(
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
