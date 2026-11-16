//! Loads agent definitions and validates their model and executable-tool references.

use std::{collections::BTreeMap, path::Path};

use agent_config::{ConfigError, read_toml_dir};
use agent_models::Registry as ModelRegistry;

use crate::{AgentDefinition, ToolRegistry};

#[derive(Default)]
pub struct AgentRegistry {
    agents: BTreeMap<String, AgentDefinition>,
}

impl AgentRegistry {
    pub fn load(data_dir: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let data_dir = data_dir.as_ref();
        let loaded = read_toml_dir::<AgentDefinition>(data_dir)?;

        if loaded.is_empty() {
            return Err(ConfigError::no_files("agent", data_dir));
        }

        let mut registry = Self::default();

        for (path, definition) in loaded {
            if definition.name.trim().is_empty() {
                return Err(ConfigError::invalid(
                    "agent",
                    Some(path),
                    "agent name cannot be empty",
                ));
            }

            if registry.agents.contains_key(&definition.name) {
                return Err(ConfigError::duplicate("agent", definition.name, Some(path)));
            }

            registry.agents.insert(definition.name.clone(), definition);
        }

        Ok(registry)
    }

    /// Load and validate all agents against the configured models and registered tools.
    pub fn load_validated(
        data_dir: impl AsRef<Path>,
        models: &ModelRegistry,
        tools: &ToolRegistry,
    ) -> Result<Self, ConfigError> {
        let registry = Self::load(data_dir)?;
        registry.validate_references(models, tools)?;
        Ok(registry)
    }

    /// Check all cross-references and report every issue in one structured error.
    ///
    /// Model checks inspect provider configuration only; they do not require API
    /// credentials. Tool checks require an executable implementation to have
    /// been registered in the runtime tool registry.
    pub fn validate_references(
        &self,
        models: &ModelRegistry,
        tools: &ToolRegistry,
    ) -> Result<(), ConfigError> {
        let mut issues = Vec::new();

        for agent in self.agents.values() {
            if !models.contains_model(&agent.model) {
                issues.push(format!(
                    "Agent '{}' references unknown model '{}'; use a configured provider/model from data/providers/*.toml",
                    agent.name, agent.model
                ));
            }

            let mut seen_tools = std::collections::HashSet::new();
            for tool_name in &agent.tools {
                if !seen_tools.insert(tool_name) {
                    issues.push(format!(
                        "Agent '{}' lists tool '{}' more than once",
                        agent.name, tool_name
                    ));
                } else if !tools.contains(tool_name) {
                    issues.push(format!(
                        "Agent '{}' references tool '{}', but no executable implementation is registered",
                        agent.name, tool_name
                    ));
                }
            }
        }

        if issues.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::invalid(
                "agent references",
                None::<&Path>,
                issues.join("; "),
            ))
        }
    }

    pub fn get(&self, name: &str) -> Option<&AgentDefinition> {
        self.agents.get(name)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &AgentDefinition> {
        self.agents.values()
    }
}
