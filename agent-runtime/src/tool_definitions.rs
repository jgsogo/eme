use std::{collections::BTreeMap, path::Path};

use agent_config::{ConfigError, read_toml_dir};

use crate::ToolDefinition;

#[derive(Default)]
pub struct ToolDefinitionRegistry {
    definitions: BTreeMap<String, ToolDefinition>,
}

impl ToolDefinitionRegistry {
    pub fn load(data_dir: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let data_dir = data_dir.as_ref();
        let loaded = read_toml_dir::<ToolDefinition>(data_dir)?;

        if loaded.is_empty() {
            return Err(ConfigError::no_files("tool definition", data_dir));
        }

        let mut registry = Self::default();

        for (path, definition) in loaded {
            if definition.name.trim().is_empty() {
                return Err(ConfigError::invalid(
                    "tool definition",
                    Some(path),
                    "tool name cannot be empty",
                ));
            }

            let name = definition.name.clone();
            if registry.definitions.contains_key(&name) {
                return Err(ConfigError::duplicate(
                    "tool definition",
                    name,
                    Some(path),
                ));
            }

            registry.definitions.insert(name, definition);
        }

        Ok(registry)
    }

    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.definitions.get(name)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &ToolDefinition> {
        self.definitions.values()
    }
}
