use agent_models::{
    registry::Registry,
    types::{ConversationMessage, ConversationRequest},
};

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "model-playground")]
#[command(about = "Experiment with different LLM providers")]
struct Args {
    /// Data directory
    #[arg(short, long)]
    data_dir: std::path::PathBuf,

    /// Model identifier: provider/model
    #[arg(short, long)]
    model: String,

    /// Prompt to send
    prompt: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let args = Args::parse();
    tracing::debug!(model = %args.model, "executing prompt");

    let registry = Registry::load(&args.data_dir)?;
    let client = registry.resolve_model(&args.model)?;

    let request = ConversationRequest {
        system: None,
        messages: vec![ConversationMessage::User {
            content: args.prompt,
        }],
        tools: vec![],
        temperature: Some(0.7),
        max_tokens: Some(1024),
    };

    let response = client.complete_conversation(request).await?;

    if let Some(content) = response.content {
        println!("{content}");
    }

    if !response.tool_calls.is_empty() {
        eprintln!("\n--- Tool calls ---");
        for call in &response.tool_calls {
            eprintln!("{}: {}", call.name, call.arguments);
        }
    }

    if let Some(usage) = response.usage {
        eprintln!("\n--- Usage ---");
        eprintln!("Input tokens: {}", usage.input_tokens);
        eprintln!("Output tokens: {}", usage.output_tokens);
    }

    Ok(())
}
