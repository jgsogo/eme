//! Executable entry point for the agent platform HTTP service.

use std::{net::IpAddr, path::PathBuf, sync::Arc};

use actix_web::{App, HttpServer, web};
use agent_models::registry::Registry as ModelRegistry;
use agent_runtime::{
    Agent, AgentRegistry, AgentRuntime, AllowListPolicy, ModelAdapter, ToolDispatcher,
    ToolRegistry,
};
use agent_service::{AgentRuntimeExecutor, AgentRuntimeResolver, ResolvedAgent, RunManager};
use anyhow::{Context, Result};
use clap::Parser;

/// Start the agent platform HTTP server.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Directory containing providers/, agents/, and tool data.
    #[arg(long, default_value = "data")]
    data_dir: PathBuf,

    /// Address on which the HTTP server listens.
    #[arg(long, default_value = "127.0.0.1")]
    host: IpAddr,

    /// TCP port on which the HTTP server listens.
    #[arg(long, default_value_t = 8080)]
    port: u16,

    /// Maximum number of agent runs executing concurrently.
    #[arg(long, default_value_t = 4)]
    max_concurrent_runs: usize,

    /// Maximum number of runs waiting in the execution queue.
    #[arg(long, default_value_t = 100)]
    max_queued_runs: usize,
}

/// Resolves configured agent names into concrete runtime instances.
struct ConfiguredAgentResolver {
    agents: AgentRegistry,
    models: ModelRegistry,
    tools: Arc<ToolRegistry>,
}

impl ConfiguredAgentResolver {
    fn load(data_dir: &std::path::Path) -> Result<Self> {
        let models = ModelRegistry::load(data_dir).context("loading model providers")?;

        let mut tools = ToolRegistry::new();
        alexa_tool::register_tools(&mut tools).context("registering Alexa tools")?;
        let tools = Arc::new(tools);

        let agents = AgentRegistry::load_validated(data_dir.join("agents"), &models, tools.as_ref())
            .context("loading and validating agent definitions")?;

        Ok(Self {
            agents,
            models,
            tools,
        })
    }
}

impl AgentRuntimeResolver for ConfiguredAgentResolver {
    fn resolve(&self, agent_name: &str) -> Result<ResolvedAgent, String> {
        let definition = self
            .agents
            .get(agent_name)
            .ok_or_else(|| format!("unknown agent '{agent_name}'"))?;

        let client = self
            .models
            .resolve_model(&definition.model)
            .map_err(|error| error.to_string())?;

        let agent = Agent::from(definition);
        let model = Arc::new(ModelAdapter::new(client));
        let tools: Arc<dyn ToolDispatcher> = self.tools.clone();
        let policy = Arc::new(AllowListPolicy::new(agent.tool_names.clone()));
        let runtime = Arc::new(AgentRuntime::new(model, tools, policy));

        Ok(ResolvedAgent { agent, runtime })
    }
}

#[actix_web::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let resolver = Arc::new(
        ConfiguredAgentResolver::load(&args.data_dir).with_context(|| {
            format!(
                "loading platform configuration from {}",
                args.data_dir.display()
            )
        })?,
    );
    let executor = Arc::new(AgentRuntimeExecutor::new(resolver));
    let manager = RunManager::new(executor, args.max_concurrent_runs, args.max_queued_runs)
        .context("configuring run manager")?;

    let bind_address = (args.host, args.port);
    println!(
        "Agent platform listening on http://{}:{}",
        bind_address.0, bind_address.1
    );

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(manager.clone()))
            .service(agent_server::runs_api())
    })
    .bind(bind_address)
    .with_context(|| format!("binding HTTP server to {}:{}", args.host, args.port))?
    .run()
    .await
    .context("running HTTP server")
}
