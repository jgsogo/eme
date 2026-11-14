use std::{collections::BTreeMap, fs, path::Path};

use crate::AgentDefinition;

#[derive(Default)]
pub struct AgentRegistry {
    agents: BTreeMap<String, AgentDefinition>,
}

impl AgentRegistry {
    pub fn load(data_dir: impl AsRef<Path>) -> Result<Self, String> {
        let data_dir = data_dir.as_ref();

        let entries = fs::read_dir(data_dir).map_err(|error| {
            format!(
                "Failed to read agent directory {}: {error}",
                data_dir.display()
            )
        })?;

        let mut registry = Self::default();

        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();

            if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                continue;
            }

            let contents = fs::read_to_string(&path)
                .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;

            let definition: AgentDefinition = toml::from_str(&contents)
                .map_err(|error| format!("Invalid agent definition {}: {error}", path.display()))?;

            if definition.name.trim().is_empty() {
                return Err(format!("Agent name cannot be empty in {}", path.display()));
            }

            if registry.agents.contains_key(&definition.name) {
                return Err(format!("Agent '{}' is already registered", definition.name));
            }

            registry.agents.insert(definition.name.clone(), definition);
        }

        if registry.agents.is_empty() {
            return Err("No agent definitions found".into());
        }

        Ok(registry)
    }

    pub fn get(&self, name: &str) -> Option<&AgentDefinition> {
        self.agents.get(name)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &AgentDefinition> {
        self.agents.values()
    }
}
