
use clap::Parser;
use agent_models::registry::resolve_model;
use anyhow::Result;
use agent_models::types::CompletionRequest;

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

    Ok(())
}
