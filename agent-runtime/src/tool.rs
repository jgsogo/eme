use std::{collections::BTreeMap, path::Path, sync::Arc};

use agent_config::{ConfigError, read_toml};
use async_trait::async_trait;
use serde_json::Value;

use crate::{ErrorCategory, ToolDefinition};

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Unknown tool: {0}")]
    UnknownTool(String),

    #[error("Invalid arguments for tool '{tool_name}': {errors:?}")]
    InvalidArguments {
        tool_name: String,
        errors: Vec<String>,
    },

    #[error("Tool configuration failed: {0}")]
    Configuration(String),

    #[error("Tool execution failed: {0}")]
    Execution(String),
}

impl ToolError {
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::Configuration(_) => ErrorCategory::Configuration,
            Self::UnknownTool(_) | Self::InvalidArguments { .. } | Self::Execution(_) => {
                ErrorCategory::Tool
            }
        }
    }
}

/// Executes tools requested by the model.
#[async_trait]
pub trait ToolDispatcher: Send + Sync {
    fn definition(&self, name: &str) -> Option<ToolDefinition> {
        let _ = name;
        None
    }
    async fn execute(&self, name: &str, arguments: Value) -> Result<String, ToolError>;
}

/// An executable tool.
#[async_trait]
pub trait Tool: Send + Sync {
    async fn execute(&self, arguments: Value) -> Result<String, ToolError>;
}

/// In-memory registry of executable tools.
#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, RegisteredTool>,
}

struct RegisteredTool {
    definition: ToolDefinition,
    implementation: Arc<dyn Tool>,
    validator: jsonschema::Validator,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load a tool definition from TOML and register it with its implementation.
    ///
    /// This is the standard registration path for tool integration crates: the
    /// definition is parsed, its JSON Schema is compiled, and the implementation
    /// is bound to the definition in one operation.
    pub fn register_from_file(
        &mut self,
        definition_path: impl AsRef<Path>,
        implementation: impl Tool + 'static,
    ) -> Result<(), ConfigError> {
        let definition_path = definition_path.as_ref();
        let definition = read_toml(definition_path)?;
        self.register_definition(definition, implementation, Some(definition_path))
    }

    /// Register an already constructed definition.
    ///
    /// This is useful for programmatic tools and tests. Integration crates
    /// should normally use `register_from_file` so metadata loading follows
    /// the same convention everywhere.
    pub fn register(
        &mut self,
        definition: ToolDefinition,
        implementation: impl Tool + 'static,
    ) -> Result<(), ConfigError> {
        self.register_definition(definition, implementation, None)
    }

    fn register_definition(
        &mut self,
        definition: ToolDefinition,
        implementation: impl Tool + 'static,
        source_path: Option<&Path>,
    ) -> Result<(), ConfigError> {
        if self.tools.contains_key(&definition.name) {
            return Err(ConfigError::duplicate(
                "tool",
                &definition.name,
                source_path.map(Path::to_path_buf),
            ));
        }

        let validator = jsonschema::validator_for(&definition.parameters).map_err(|error| {
            ConfigError::invalid(
                "tool",
                source_path.map(Path::to_path_buf),
                format!("invalid schema for '{}': {error}", definition.name),
            )
        })?;

        self.tools.insert(
            definition.name.clone(),
            RegisteredTool {
                definition,
                implementation: Arc::new(implementation),
                validator,
            },
        );

        Ok(())
    }

    /// Returns whether an executable implementation is registered for the given name.
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    pub fn definition(&self, name: &str) -> Option<ToolDefinition> {
        self.tools.get(name).map(|tool| tool.definition.clone())
    }

    /// Returns the definitions of all registered tools.
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|tool| tool.definition.clone())
            .collect()
    }
}

#[async_trait]
impl ToolDispatcher for ToolRegistry {
    fn definition(&self, name: &str) -> Option<ToolDefinition> {
        self.definition(name)
    }

    async fn execute(&self, name: &str, arguments: Value) -> Result<String, ToolError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::UnknownTool(name.into()))?;

        // Validate the arguments coming from the model, it is not a trusted source!
        let errors = tool
            .validator
            .iter_errors(&arguments)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();

        if !errors.is_empty() {
            return Err(ToolError::InvalidArguments {
                tool_name: name.to_owned(),
                errors,
            });
        }

        tool.implementation
            .execute(arguments)
            .await
            .map_err(ToolError::Execution)
    }
}

#[cfg(test)]
mod registration_tests {
    use std::{fs, path::PathBuf};

    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .join(format!("tool-registration-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn file(&self, name: &str) -> PathBuf {
            self.0.join(name)
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

    #[test]
    fn loads_definition_and_registers_implementation_from_file() {
        let directory = TestDirectory::new();
        let path = directory.file("example.toml");
        fs::write(
            &path,
            r#"
name = "example"
description = "An example tool"

[parameters]
type = "object"
additionalProperties = false
"#,
        )
        .unwrap();

        let mut registry = ToolRegistry::new();
        registry.register_from_file(&path, NoopTool).unwrap();

        assert!(registry.contains("example"));
        assert_eq!(
            registry.definition("example").unwrap().description,
            "An example tool"
        );
    }

    #[test]
    fn schema_errors_keep_the_definition_path() {
        let directory = TestDirectory::new();
        let path = directory.file("invalid-schema.toml");
        fs::write(
            &path,
            r#"
name = "invalid"
description = "Invalid schema"

[parameters]
type = "not-a-json-schema-type"
"#,
        )
        .unwrap();

        let mut registry = ToolRegistry::new();
        let error = registry.register_from_file(&path, NoopTool).unwrap_err();

        assert!(matches!(
            error,
            ConfigError::Invalid { path: Some(error_path), .. } if error_path == path
        ));
    }

    #[test]
    fn duplicate_file_registration_reports_the_second_definition_path() {
        let directory = TestDirectory::new();
        let first = directory.file("first.toml");
        let second = directory.file("second.toml");
        let definition = r#"
name = "duplicate"
description = "Duplicate tool"

[parameters]
type = "object"
"#;
        fs::write(&first, definition).unwrap();
        fs::write(&second, definition).unwrap();

        let mut registry = ToolRegistry::new();
        registry.register_from_file(&first, NoopTool).unwrap();
        let error = registry.register_from_file(&second, NoopTool).unwrap_err();

        assert!(matches!(
            error,
            ConfigError::Duplicate { path: Some(error_path), .. } if error_path == second
        ));
    }
}
