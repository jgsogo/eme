use std::{path::PathBuf, sync::Arc};

use agent_runtime::{
    Agent, AgentRegistry, AgentRuntime, AllowListPolicy, CancellationToken, ModelAdapter,
    ToolRegistry,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data");

    let agents = AgentRegistry::load(data_dir.join("agents"))?;
    let agent_definition = agents
        .get("assistant")
        .ok_or("Agent 'assistant' not found")?;

    let mut tools = ToolRegistry::new();
    alexa_tool::register_tools(&mut tools)?;

    let model_registry = agent_models::registry::Registry::load(&data_dir)?;

    let model = model_registry.resolve_model(&agent_definition.model)?;
    let model = Arc::new(ModelAdapter::new(model));

    let agent = Agent::from(agent_definition);

    let allowed_tools = agent.tool_names.clone();

    let runtime = AgentRuntime::new(
        model,
        Arc::new(tools),
        Arc::new(AllowListPolicy::new(allowed_tools)),
    );

    let result = runtime
        .run(
            &agent,
            "Send a message to my Alexa device",
            CancellationToken::new(),
        )
        .await?;

    Ok(())
}
