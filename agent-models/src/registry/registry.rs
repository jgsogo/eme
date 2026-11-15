use crate::{
    client::ConversationClient,
    error::ModelError,
    models::{ModelCapability, ModelConfig},
    providers::{GeminiClient, OpenAICompatibleClient},
};
use agent_config::{ConfigError, read_toml_dir};
use std::{collections::BTreeMap, path::Path, sync::Arc};

use super::provider_config::ProviderConfig;

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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::Registry;
    use crate::error::ModelError;
    use agent_config::ConfigError;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "provider-registry-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn case(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir_all(path.join("providers")).unwrap();
            path
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_provider(root: &Path, file: &str, name: &str, client: &str) {
        fs::write(
            root.join("providers").join(file),
            format!(
                r#"
name = "{name}"
client = "{client}"
api_key_env = "TEST_API_KEY"

[[models]]
name = "test-model"
capabilities = ["chat"]
"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn loads_valid_providers_and_rejects_invalid_registry_configuration() {
        let directory = TestDirectory::new();

        // Valid provider configuration: model lookup must not require credentials.
        let valid = directory.case("valid");
        write_provider(&valid, "provider.toml", "test", "gemini");
        let registry = Registry::load(&valid).unwrap();
        assert!(registry.contains_model("test/test-model"));
        assert!(!registry.contains_model("test/missing-model"));

        // Duplicate provider identifiers are rejected across files.
        let duplicate = directory.case("duplicate");
        write_provider(&duplicate, "a.toml", "same", "gemini");
        write_provider(&duplicate, "b.toml", "same", "gemini");
        assert!(matches!(
            Registry::load(&duplicate),
            Err(ConfigError::Duplicate { .. })
        ));

        // Unsupported provider clients are invalid definitions.
        let invalid = directory.case("invalid");
        write_provider(&invalid, "provider.toml", "test", "unsupported");
        assert!(matches!(
            Registry::load(&invalid),
            Err(ConfigError::Invalid { .. })
        ));

        // A missing providers directory is reported as a filesystem error.
        assert!(matches!(
            Registry::load(directory.0.join("missing")),
            Err(ConfigError::Io { .. })
        ));

        // Resolving a declared model still distinguishes missing credentials.
        assert!(matches!(
            registry.resolve_model("test/test-model"),
            Err(ModelError::MissingApiKey(_))
        ));
    }
}
