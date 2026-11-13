use agent_runtime::{Tool, ToolDefinition, ToolRegistry};
use async_trait::async_trait;
use serde_json::{Value, json};

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

    tools.register(
        ToolDefinition {
            name: "calculator".into(),
            description: "Multiplies two integers.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "a": { "type": "integer" },
                    "b": { "type": "integer" }
                },
                "required": ["a", "b"],
                "additionalProperties": false
            }),
        },
        Calculator,
    )?;

    println!("Registered tools:");
    for definition in tools.definitions() {
        println!("- {}: {}", definition.name, definition.description);
    }

    Ok(())
}
