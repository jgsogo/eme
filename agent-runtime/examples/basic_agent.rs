use std::path::PathBuf;

use agent_runtime::{AgentRegistry, ToolRegistry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../data/agents");

    let agents = AgentRegistry::load(data_dir)?;

    let agent = agents
        .get("assistant")
        .ok_or("Agent 'assistant' not found")?;

    let mut tools = ToolRegistry::new();

    alexa_tool::register_tools(&mut tools)?;

    // For now, inspect the definition and registered tools.
    println!("Agent: {}", agent.name);
    println!("Model: {}", agent.model);
    println!("Available tools: {:?}", agent.tools);

    Ok(())
}
