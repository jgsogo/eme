use std::path::PathBuf;

use agent_runtime::{Tool, ToolError, ToolRegistry};
use async_trait::async_trait;
use serde_json::Value;

struct Calculator;

#[async_trait]
impl Tool for Calculator {
    async fn execute(&self, arguments: Value) -> Result<String, ToolError> {
        let a = arguments["a"]
            .as_i64()
            .ok_or_else(|| ToolError::InvalidArguments {
                tool_name: "calculator".into(),
                errors: vec!["Missing or invalid argument: a".into()],
            })?;

        let b = arguments["b"]
            .as_i64()
            .ok_or_else(|| ToolError::InvalidArguments {
                tool_name: "calculator".into(),
                errors: vec!["Missing or invalid argument: b".into()],
            })?;

        let result = a
            .checked_mul(b)
            .ok_or_else(|| ToolError::Execution("Integer multiplication overflow".into()))?;

        Ok(result.to_string())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let mut tools = ToolRegistry::new();

    let definition_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/data/calculator.toml");
    tools.register_from_file(definition_path, Calculator)?;

    println!("Registered tools:");
    for definition in tools.definitions() {
        println!("- {}: {}", definition.name, definition.description);
    }

    Ok(())
}
