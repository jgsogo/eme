use std::{net::IpAddr,path::PathBuf};
use clap::Parser;

/// Start the agent platform HTTP server.
#[derive(Debug, Parser)]
#[command(version, about)]
pub(super) struct Args {
    /// Directory containing providers/, agents/, tasks/, and tool data.
    #[arg(long, default_value = "data")]
    pub data_dir: PathBuf,

    /// Address on which the HTTP server listens.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: IpAddr,

    /// TCP port on which the HTTP server listens.
    #[arg(long, default_value_t = 8080)]
    pub port: u16,

    /// Maximum number of agent runs executing concurrently.
    #[arg(long, default_value_t = 4)]
    pub max_concurrent_runs: usize,

    /// Agent used for prompts received through Telegram.
    #[arg(long, default_value = "assistant")]
    pub telegram_agent: String,

    /// Maximum number of runs waiting in the execution queue.
    #[arg(long, default_value_t = 100)]
    pub max_queued_runs: usize,
}
