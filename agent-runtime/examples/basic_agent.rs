
use std::{path::PathBuf, sync::Arc};

use agent_runtime::{
    Agent, AgentModel, AgentRuntime, AllowListPolicy, CancellationToken, ModelAdapter,
    Tool, ToolDefinition, ToolRegistry,
};
use agent_models::registry::Registry;

use async_trait::async_trait;
use serde_json::{json, Value};

struct AddTool;

#[async_trait]
impl Tool for AddTool {
    async fn execute(&self, arguments: Value) -> Result<String, String> {
        let a = arguments["a"]
            .as_f64()
            .ok_or_else(|| "Missing or invalid argument 'a'".to_owned())?;

        let b = arguments["b"]
            .as_f64()
            .ok_or_else(|| "Missing or invalid argument 'b'".to_owned())?;

        let result = a + b;

        println!("Executing add: {a} + {b} = {result}");

        Ok(result.to_string())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);

    let model_name = args
        .next()
        .or_else(|| std::env::var("AGENT_MODEL").ok())
        .ok_or_else(|| {
            std::io::Error::other(
                "Usage: basic_agent <provider/model> [data-dir]",
            )
        })?;

    let data_dir = args
        .next()
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("AGENT_DATA_DIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("data"));

    // Load the model provider and resolve the requested model.
    let registry = Registry::load(&data_dir)?;
    let client = registry.resolve_model(&model_name)?;

    let model: Arc<dyn AgentModel> = Arc::new(ModelAdapter::new(client));

    // Register the tools available to the agent.
    let mut tool_registry = ToolRegistry::new();

    tool_registry
        .register(
            ToolDefinition {
                name: "add".into(),
                description: "Add two numbers and return their sum.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "a": { "type": "number" },
                        "b": { "type": "number" }
                    },
                    "required": ["a", "b"],
                }),
            },
            AddTool,
        )
        .map_err(std::io::Error::other)?;

    let tools = Arc::new(tool_registry);
    let policy = Arc::new(AllowListPolicy::new(["add"]));

    let runtime = AgentRuntime::new(model, tools, policy);

    let agent = Agent {
        id: "basic-agent".into(),
        system_prompt: Some(
            "You are a helpful assistant. When asked to perform arithmetic, \
             you must use the available tools. Do not calculate the result yourself."
                .into(),
        ),
        tool_names: vec!["add".into()],
        max_iterations: 5,
    };

    let input = "What is 1234 + 5678? Use the add tool.";

    println!("Model: {model_name}");
    println!("Input: {input}\n");

    let result = runtime
        .run(&agent, input, CancellationToken::new())
        .await
        .map_err(|failure| std::io::Error::other(format!("{failure:?}")))?;

    println!("\n--- Run result ---");
    println!("Status: {:?}", result.status);
    println!("Iterations: {}", result.iterations);
    println!(
        "Response: {}",
        result.response.as_deref().unwrap_or("<no response>")
    );

    Ok(())
}