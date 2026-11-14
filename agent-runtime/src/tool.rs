use std::sync::Arc;

use crate::ToolDefinition;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Unknown tool: {0}")]
    UnknownTool(String),

    #[error("Invalid arguments for tool '{tool_name}': {errors:?}")]
    InvalidArguments {
        tool_name: String,
        errors: Vec<String>,
    },

    #[error("Tool execution failed: {0}")]
    Execution(String),
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
    async fn execute(&self, arguments: Value) -> Result<String, String>;
}

/// In-memory registry of executable tools.
#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, RegisteredTool>, // use a BTreeMap to keep order deterministic (it will help debugging)
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

    pub fn register(
        &mut self,
        definition: ToolDefinition,
        implementation: impl Tool + 'static,
    ) -> Result<(), String> {
        if self.tools.contains_key(&definition.name) {
            return Err(format!("Tool '{}' is already registered", definition.name));
        }

        let validator = jsonschema::validator_for(&definition.parameters)
            .map_err(|error| format!("Invalid schema for tool '{}': {error}", definition.name))?;

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
