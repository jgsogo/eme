use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use rabel_tool::{CatalogueSearchResult, RabelClient};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(
    name = "rabel",
    about = "Inspect the RABEL / AbsysNet public catalogue"
)]
struct Args {
    /// RABEL subcatalogue identifier (defaults to Salamanca Public Library).
    #[arg(long, default_value = "SAL/BPSA", global = true)]
    subcatalog: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Search the catalogue and print either parsed JSON or the raw search HTML.
    Search {
        query: String,
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Search, select a result, and print its parsed details or raw record HTML.
    Record {
        query: String,
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,
        /// Zero-based result index from the search response.
        #[arg(long, default_value_t = 0)]
        index: usize,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    Html,
}

#[derive(Serialize)]
struct RecordOutput<'a> {
    search_result: &'a CatalogueSearchResult,
    details: rabel_tool::CatalogueItem,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rabel: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let args = Args::parse();
    let client = RabelClient::for_subcatalog(&args.subcatalog)?;

    match args.command {
        Command::Search {
            query,
            format,
            limit,
        } => match format {
            OutputFormat::Html => print!("{}", client.search_raw(&query).await?),
            OutputFormat::Json => {
                let results = client.search(&query, limit).await?;
                println!("{}", serde_json::to_string_pretty(&results)?);
            }
        },
        Command::Record {
            query,
            format,
            index,
        } => {
            let results = client.search(&query, 20).await?;
            let result = results.get(index).ok_or_else(|| {
                anyhow::anyhow!(
                    "result index {index} is out of range ({} results)",
                    results.len()
                )
            })?;
            match format {
                OutputFormat::Html => print!("{}", client.get_item_raw(&result.record_ref).await?),
                OutputFormat::Json => {
                    let details = client.get_item(&result.record_ref).await?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&RecordOutput {
                            search_result: result,
                            details,
                        })?
                    );
                }
            }
        }
    }
    Ok(())
}
