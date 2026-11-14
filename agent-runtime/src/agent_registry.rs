use std::{collections::BTreeMap, fs, path::Path};

use agent_models::registry::Registry as ModelRegistry;

use crate::{AgentDefinition, ToolRegistry};

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

    /// Load and validate all agents against the configured models and registered tools.
    pub fn load_validated(
        data_dir: impl AsRef<Path>,
        models: &ModelRegistry,
        tools: &ToolRegistry,
    ) -> Result<Self, String> {
        let registry = Self::load(data_dir)?;
        registry
            .validate_references(models, tools)
            .map_err(|issues| {
                format!(
                    "Agent configuration validation failed:\n- {}",
                    issues.join("\n- ")
                )
            })?;
        Ok(registry)
    }

    /// Check all cross-references and return every issue in one pass.
    ///
    /// Model checks inspect provider configuration only; they do not require API
    /// credentials. Tool checks require an executable implementation to have
    /// been registered in the runtime tool registry.
    pub fn validate_references(
        &self,
        models: &ModelRegistry,
        tools: &ToolRegistry,
    ) -> Result<(), Vec<String>> {
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
            Err(issues)
        }
    }

    pub fn get(&self, name: &str) -> Option<&AgentDefinition> {
        self.agents.get(name)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &AgentDefinition> {
        self.agents.values()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use agent_models::registry::Registry as ModelRegistry;
    use async_trait::async_trait;
    use serde_json::{Value, json};

    use crate::{Tool, ToolDefinition, ToolRegistry};

    use super::AgentRegistry;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("agent-registry-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(path.join("agents")).unwrap();
            fs::create_dir_all(path.join("providers")).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct NoopTool;

    #[async_trait]
    impl Tool for NoopTool {
        async fn execute(&self, _arguments: Value) -> Result<String, String> {
            Ok(String::new())
        }
    }

    fn fixture(model: &str, tool_names: &[&str]) -> (TestDirectory, ModelRegistry, AgentRegistry) {
        let directory = TestDirectory::new();
        fs::write(
            directory.path().join("providers/test.toml"),
            r#"
name = "test"
client = "gemini"
api_key_env = "TEST_API_KEY"

[[models]]
name = "test-model"
capabilities = ["chat"]
"#,
        )
        .unwrap();

        let tools = tool_names
            .iter()
            .map(|name| format!("    \"{name}\",\n"))
            .collect::<String>();
        fs::write(
            directory.path().join("agents/test.toml"),
            format!(
                r#"
name = "test-agent"
description = "Test agent"
model = "{model}"
instructions = "Test instructions"
tools = [
{tools}]
"#
            ),
        )
        .unwrap();

        let models = ModelRegistry::load(directory.path()).unwrap();
        let agents = AgentRegistry::load(directory.path().join("agents")).unwrap();
        (directory, models, agents)
    }

    fn registered_tool() -> ToolRegistry {
        let mut tools = ToolRegistry::new();
        tools
            .register(
                ToolDefinition {
                    name: "test_tool".into(),
                    description: "A test tool".into(),
                    parameters: json!({"type": "object"}),
                },
                NoopTool,
            )
            .unwrap();
        tools
    }

    #[test]
    fn validates_existing_model_and_registered_tool() {
        let (directory, models, agents) = fixture("test/test-model", &["test_tool"]);
        let tools = registered_tool();

        assert!(agents.validate_references(&models, &tools).is_ok());
        assert!(
            AgentRegistry::load_validated(directory.path().join("agents"), &models, &tools).is_ok()
        );
    }

    #[test]
    fn reports_all_missing_references_together() {
        let (_directory, models, agents) = fixture("missing/model", &["missing_tool"]);
        let tools = ToolRegistry::new();
        let issues = agents.validate_references(&models, &tools).unwrap_err();

        assert_eq!(issues.len(), 2);
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("unknown model 'missing/model'"))
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("tool 'missing_tool'"))
        );
    }

    #[test]
    fn reports_duplicate_tool_references() {
        let (_directory, models, agents) = fixture("test/test-model", &["test_tool", "test_tool"]);
        let tools = registered_tool();
        let issues = agents.validate_references(&models, &tools).unwrap_err();

        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("lists tool 'test_tool' more than once"));
    }
}
