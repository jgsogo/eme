use std::{path::PathBuf, sync::Arc};

use agent_runtime::{
    Agent, AgentRegistry, AgentRuntime, AllowListPolicy, CancellationToken, ModelAdapter,
    ToolContext, ToolRegistry,
};
use anyhow::{Context, Result};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data");

    let mut tools = ToolRegistry::new();
    alexa_tool::register_tools(&mut tools).context("registering Alexa tools")?;
    telegram_tool::register_tools(&mut tools, None).context("registering Telegram tools")?;
    meteosource_tool::register_tools(&mut tools).context("registering Meteosource tools")?;
    tavily_tool::register_tools(&mut tools).context("registering Tavily tools")?;
    google_tool::register_tools(&mut tools).context("registering Google tools")?;

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
        return Err(
            "Usage: basic_agent <prompt>\nExample: basic_agent \"Ask Alexa to say Hello\"".into(),
        );
    }

    let result = runtime
        .run(
            uuid::Uuid::new_v4(),
            &agent,
            ToolContext::default(),
            Vec::new(),
            input,
            CancellationToken::new(),
        )
        .await?;

    println!("Status: {:?}", result.status);
    println!("Iterations: {}", result.iterations);

    for event in &result.events {
        match &event.kind {
            agent_runtime::RunEventKind::ToolExecutionStarted { name, .. } => {
                println!("Executing tool: {name}");
            }
            agent_runtime::RunEventKind::ToolExecutionFinished {
                name,
                error_category,
                ..
            } => {
                println!("Tool {name} finished (error: {error_category:?})");
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
