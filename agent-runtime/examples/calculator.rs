use std::path::PathBuf;

use agent_runtime::{Tool, ToolRegistry};
use async_trait::async_trait;
use serde_json::Value;

struct Calculator;

#[async_trait]
impl Tool for Calculator {
    async fn execute(&self, arguments: Value) -> Result<String, String> {
        let a = arguments["a"]
            .as_i64()
            .ok_or("Missing or invalid argument: a")?;

        let b = arguments["b"]
            .as_i64()
            .ok_or("Missing or invalid argument: b")?;

        let result = a.checked_mul(b).ok_or("Integer multiplication overflow")?;

        Ok(result.to_string())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
