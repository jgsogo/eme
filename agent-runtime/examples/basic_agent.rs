use std::{path::PathBuf, sync::Arc};

use agent_runtime::{
    Agent, AgentRegistry, AgentRuntime, AllowListPolicy, CancellationToken, ModelAdapter,
    ToolRegistry,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data");

    let mut tools = ToolRegistry::new();
    alexa_tool::register_tools(&mut tools)?;

    let model_registry = agent_models::registry::Registry::load(&data_dir)?;
    let agents = AgentRegistry::load_validated(data_dir.join("agents"), &model_registry, &tools)?;
    let agent_definition = agents
        .get("assistant")
        .ok_or("Agent 'assistant' not found")?;

    let model = model_registry.resolve_model(&agent_definition.model)?;
    let model = Arc::new(ModelAdapter::new(model));

    let agent = Agent::from(agent_definition);
    let allowed_tools = agent.tool_names.clone();

    let runtime = AgentRuntime::new(
        model,
        Arc::new(tools),
        Arc::new(AllowListPolicy::new(allowed_tools)),
    );

    let input = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if input.is_empty() {
        return Err("Usage: basic_agent <prompt>\nExample: basic_agent \"Ask Alexa to say Hello\""
            .into());
    }

    let result = runtime.run(&agent, input, CancellationToken::new()).await?;

    println!("Status: {:?}", result.status);
    println!("Iterations: {}", result.iterations);

    for event in &result.events {
        match &event.kind {
            agent_runtime::RunEventKind::ToolExecutionStarted { name, .. } => {
                println!("Executing tool: {name}");
            }
            agent_runtime::RunEventKind::ToolExecutionFinished { name, is_error, .. } => {
                println!("Tool {name} finished (error: {is_error})");
            }
            agent_runtime::RunEventKind::ModelResponded { tool_calls } => {
                println!("Model responded (tool calls: {tool_calls})");
            }
            _ => {}
        }
    }

    for message in &result.messages {
        if let agent_runtime::Message::Tool {
            name,
            content,
            is_error: true,
            ..
        } = message
        {
            eprintln!("Tool '{name}' failed: {content}");
        }
    }

    if let Some(response) = result.response {
        println!("\nAgent: {response}");
    }

    Ok(())
}
