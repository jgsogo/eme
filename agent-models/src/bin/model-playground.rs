use agent_models::registry::resolve_model;
use agent_models::types::CompletionRequest;
use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "model-playground")]
#[command(about = "Experiment with different LLM providers")]
struct Args {
    /// Model identifier: provider/model
    #[arg(short, long)]
    model: String,

    /// Prompt to send
    prompt: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();
    log::debug!("Execute prompt using model '{}'", &args.model);

    let client = resolve_model(&args.model)?;
    let request = CompletionRequest::new(args.prompt);

    let response = client.complete(request).await?;

    println!("{}", response.content);

    if let Some(usage) = response.usage {
        eprintln!("\n--- Usage ---");
        eprintln!("Input tokens: {}", usage.input_tokens);
        eprintln!("Output tokens: {}", usage.output_tokens);
    }

    Ok(())
}
