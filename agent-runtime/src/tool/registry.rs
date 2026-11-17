//! In-memory registration, schema validation, and execution of tool implementations.

use super::{RegisteredTool, Tool, ToolDispatcher, ToolError};
use crate::{ToolContext, ToolDefinition};
use agent_config::{ConfigError, read_toml};
use async_trait::async_trait;
use serde_json::Value;
use std::{collections::BTreeMap, path::Path, sync::Arc};

/// In-memory registry of executable tools.
#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, RegisteredTool>,
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

    async fn execute(
        &self,
        context: &ToolContext,
        name: &str,
        arguments: Value,
    ) -> Result<String, ToolError> {
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

        tool.implementation.execute(context, arguments).await
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
        async fn execute(
            &self,
            _context: &ToolContext,
            _arguments: Value,
        ) -> Result<String, ToolError> {
            Ok(String::new())
        }
    }

    struct ContextTool(std::sync::Arc<std::sync::Mutex<Option<(Option<String>, Option<String>)>>>);

    #[async_trait]
    impl Tool for ContextTool {
        async fn execute(
            &self,
            context: &ToolContext,
            _arguments: Value,
        ) -> Result<String, ToolError> {
            *self.0.lock().unwrap() = Some((
                context.user_id.clone(),
                context
                    .family
                    .as_ref()
                    .map(|family| family.family_id.clone()),
            ));
            Ok("context received".into())
        }
    }

    #[tokio::test]
    async fn dispatches_authenticated_context_to_tool_implementation() {
        let captured_context = std::sync::Arc::new(std::sync::Mutex::new(None));
        let mut registry = ToolRegistry::new();
        registry
            .register(
                ToolDefinition {
                    name: "context".into(),
                    description: "Reads trusted context".into(),
                    parameters: serde_json::json!({"type": "object"}),
                },
                ContextTool(captured_context.clone()),
            )
            .unwrap();

        let context = ToolContext {
            user_id: Some("user-42".into()),
            family: Some(crate::FamilyContext {
                family_id: "trusted-family".into(),
                family_name: "Home".into(),
                family_role: "owner".into(),
                relationship: "father".into(),
                members: Vec::new(),
            }),
            ..ToolContext::default()
        };
        let output = registry
            .execute(
                &context,
                "context",
                serde_json::json!({"family_id": "model-forged-family"}),
            )
            .await
            .unwrap();

        assert_eq!(output, "context received");
        assert_eq!(
            captured_context.lock().unwrap().as_ref(),
            Some(&(Some("user-42".into()), Some("trusted-family".into())))
        );

        registry
            .execute(&ToolContext::default(), "context", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(
            captured_context.lock().unwrap().as_ref(),
            Some(&(None, None))
        );
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
