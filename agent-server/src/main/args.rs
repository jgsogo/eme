//! Command-line configuration for the server process.

use std::{net::IpAddr, path::PathBuf};

use clap::Parser;

/// Start the agent platform HTTP server.
#[derive(Debug, Parser)]
#[command(version, about)]
pub(super) struct Args {
    /// Directory containing providers/, agents/, tasks/, and tool data.
    #[arg(long, default_value = "data")]
    pub(super) data_dir: PathBuf,

    /// Address on which the HTTP server listens.
    #[arg(long, default_value = "127.0.0.1")]
    pub(super) host: IpAddr,

    /// TCP port on which the HTTP server listens.
    #[arg(long, default_value_t = 8080)]
    pub(super) port: u16,

    /// Optional override for the conversation and run-history SQLite database path.
    #[arg(long)]
    pub(super) conversation_database: Option<PathBuf>,

    /// Optional override for the Meal Planner SQLite database path.
    #[arg(long)]
    pub(super) meal_planner_database: Option<PathBuf>,

    /// Maximum number of agent runs executing concurrently.
    #[arg(long, default_value_t = 4)]
    pub(super) max_concurrent_runs: usize,

    /// Agent used for prompts received through Telegram.
    #[arg(long, default_value = "assistant")]
    pub(super) telegram_agent: String,

    /// Maximum number of runs waiting in the execution queue.
    #[arg(long, default_value_t = 100)]
    pub(super) max_queued_runs: usize,
}
