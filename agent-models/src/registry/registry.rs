use crate::{
    client::ConversationClient,
    error::ModelError,
    models::{AccessTier, ModelCapability},
    providers::{GeminiClient, OpenAICompatibleClient},
};
use agent_config::{ConfigError, read_toml_dir};
use std::{collections::BTreeMap, path::Path, sync::Arc};

use super::provider_config::ProviderConfig;

/// Credential-free metadata for a configured provider and its models.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderInfo {
    pub name: String,
    pub client: String,
    pub enabled: bool,
    pub models: Vec<ProviderModelInfo>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderModelInfo {
    pub name: String,
    pub enabled: bool,
    pub access_tier: AccessTier,
    pub capabilities: Vec<ModelCapability>,
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

    /// Returns the validated provider/model metadata without exposing credentials.
    ///
    /// Disabled entries remain visible so administrative interfaces can explain
    /// the catalogue configuration; runtime selection methods exclude them.
    pub fn catalog(&self) -> Vec<ProviderInfo> {
        self.providers
            .values()
            .map(|provider| ProviderInfo {
                name: provider.name.clone(),
                client: provider.client.clone(),
                enabled: provider.enabled,
                models: provider
                    .models
                    .iter()
                    .map(|model| ProviderModelInfo {
                        name: model.name.clone(),
                        enabled: model.enabled,
                        access_tier: model.access_tier,
                        capabilities: model.capabilities.clone(),
                    })
                    .collect(),
            })
            .collect()
    }

    /// Returns whether an enabled provider/model pair is declared in the configuration.
    ///
    /// This check does not read credentials or construct a provider client.
    pub fn contains_model(&self, model_name: &str) -> bool {
        let Some((provider_name, model_name)) = model_name.split_once('/') else {
            return false;
        };

        if provider_name.is_empty() || model_name.is_empty() {
            return false;
        }

        self.providers.get(provider_name).is_some_and(|provider| {
            provider.enabled
                && provider
                    .models
                    .iter()
                    .any(|model| model.enabled && model.name == model_name)
        })
    }

    /// Returns whether one enabled model provides every requested capability.
    ///
    /// Provider names are traversed in `BTreeMap` order and models in their TOML
    /// declaration order, so the first compatible model is deterministic. This
    /// metadata-only query does not read credentials or contact providers.
    pub fn contains_compatible_model(&self, required: &[ModelCapability]) -> bool {
        self.first_compatible_model(required).is_some()
    }

    /// Returns every enabled model that provides all requested capabilities.
    ///
    /// Provider names are traversed in `BTreeMap` order and models in their TOML
    /// declaration order. The result is therefore deterministic and provider-qualified.
    /// This metadata-only query does not read credentials or contact providers.
    pub fn compatible_models(&self, required: &[ModelCapability]) -> Vec<String> {
        if required.is_empty() {
            return Vec::new();
        }

        self.providers
            .iter()
            .filter(|(_, provider)| provider.enabled)
            .flat_map(|(provider_name, provider)| {
                provider
                    .models
                    .iter()
                    .filter(move |model| {
                        model.enabled
                            && required
                                .iter()
                                .all(|capability| model.capabilities.contains(capability))
                    })
                    .map(move |model| format!("{provider_name}/{}", model.name))
            })
            .collect()
    }

    /// Finds the first enabled model that provides every requested capability.
    ///
    /// This is a convenience wrapper around deterministic candidate enumeration.
    pub fn first_compatible_model(&self, required: &[ModelCapability]) -> Option<String> {
        self.compatible_models(required).into_iter().next()
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

        if !provider.enabled {
            return Err(ModelError::ProviderDisabled(provider_name.to_string()));
        }

        let model_config = provider
            .models
            .iter()
            .find(|configured| configured.name == model)
            .ok_or_else(|| ModelError::UnsupportedModel(format!("{provider_name}/{model}")))?;

        if !model_config.enabled {
            return Err(ModelError::ModelDisabled(format!(
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
    use crate::{
        error::ModelError,
        models::{AccessTier, ModelCapability},
    };
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

        // Existing TOML files remain enabled by default for backwards compatibility.
        let valid = directory.case("valid");
        write_provider(&valid, "provider.toml", "test", "gemini");
        let valid_registry = Registry::load(&valid).unwrap();
        assert!(valid_registry.contains_model("test/test-model"));
        assert!(!valid_registry.contains_model("test/missing-model"));
        assert!(valid_registry.contains_compatible_model(&[ModelCapability::Chat]));
        assert_eq!(
            valid_registry.compatible_models(&[ModelCapability::Chat]),
            vec!["test/test-model"]
        );
        assert_eq!(
            valid_registry
                .first_compatible_model(&[ModelCapability::Chat])
                .as_deref(),
            Some("test/test-model")
        );
        assert!(!valid_registry.contains_compatible_model(&[ModelCapability::Reasoning]));
        assert_eq!(valid_registry.first_compatible_model(&[]), None);
        let catalog = valid_registry.catalog();
        assert!(catalog[0].enabled);
        assert!(catalog[0].models[0].enabled);
        assert_eq!(catalog[0].models[0].access_tier, AccessTier::Unknown);

        // Disabled providers and models stay visible in the catalogue but cannot
        // be selected for runtime execution.
        let disabled = directory.case("disabled");
        fs::write(
            disabled.join("providers/provider.toml"),
            r#"
name = "disabled"
client = "gemini"
api_key_env = "TEST_API_KEY"
enabled = false

[[models]]
name = "disabled-model"
capabilities = ["chat"]
"#,
        )
        .unwrap();
        let registry = Registry::load(&disabled).unwrap();
        assert!(!registry.contains_model("disabled/disabled-model"));
        assert!(
            registry
                .compatible_models(&[ModelCapability::Chat])
                .is_empty()
        );
        assert!(matches!(
            registry.resolve_model("disabled/disabled-model"),
            Err(ModelError::ProviderDisabled(_))
        ));
        assert!(!registry.catalog()[0].enabled);

        let disabled_model = directory.case("disabled-model");
        fs::write(
            disabled_model.join("providers/provider.toml"),
            r#"
name = "test"
client = "gemini"
api_key_env = "TEST_API_KEY"

[[models]]
name = "test-model"
enabled = false
access_tier = "free_tier"
capabilities = ["chat"]
"#,
        )
        .unwrap();
        let registry = Registry::load(&disabled_model).unwrap();
        assert!(!registry.contains_model("test/test-model"));
        assert!(
            registry
                .compatible_models(&[ModelCapability::Chat])
                .is_empty()
        );
        assert!(matches!(
            registry.resolve_model("test/test-model"),
            Err(ModelError::ModelDisabled(_))
        ));
        assert_eq!(
            registry.catalog()[0].models[0].access_tier,
            AccessTier::FreeTier
        );

        // Candidate ordering is stable across provider files and independent
        // of the order in which those files are discovered.
        let ordered = directory.case("ordered");
        write_provider(&ordered, "z.toml", "zeta", "gemini");
        write_provider(&ordered, "a.toml", "alpha", "gemini");
        let ordered_registry = Registry::load(&ordered).unwrap();
        assert_eq!(
            ordered_registry.compatible_models(&[ModelCapability::Chat]),
            vec!["alpha/test-model", "zeta/test-model"]
        );

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
            valid_registry.resolve_model("test/test-model"),
            Err(ModelError::MissingApiKey(_))
        ));
    }
}
