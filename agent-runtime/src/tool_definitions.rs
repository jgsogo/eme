use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::ToolDefinition;

#[derive(Default)]
pub struct ToolDefinitionRegistry {
    definitions: BTreeMap<String, ToolDefinition>,
}

impl ToolDefinitionRegistry {
    pub fn load(data_dir: impl AsRef<Path>) -> Result<Self, String> {
        let data_dir = data_dir.as_ref();
        let mut paths = fs::read_dir(data_dir)
            .map_err(|error| format!("Failed to read {}: {error}", data_dir.display()))?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<PathBuf>, _>>()
            .map_err(|error| format!("Failed to read tool definition entries: {error}"))?;

        // Deterministic loading order.
        paths.sort();

        let mut registry = Self::default();

        for path in paths {
            if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                continue;
            }

            let contents = fs::read_to_string(&path)
                .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;

            let definition: ToolDefinition = toml::from_str(&contents)
                .map_err(|error| format!("Invalid tool definition {}: {error}", path.display()))?;

            let name = definition.name.clone();

            if registry.definitions.contains_key(&name) {
                return Err(format!("Tool definition '{name}' is duplicated"));
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
