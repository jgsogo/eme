//! Executable entry point for the agent platform HTTP service.

use std::{net::IpAddr, path::PathBuf, sync::Arc};

mod telegram;

use actix_web::{
    App, Error, HttpResponse, HttpServer, Responder, body::{EitherBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse}, get, middleware::{self, Next}, web,
};
use agent_models::registry::Registry as ModelRegistry;
use agent_runtime::{
    Agent, AgentRegistry, AgentRuntime, AllowListPolicy, ModelAdapter, ToolDispatcher, ToolRegistry,
};
use agent_service::{
    AgentRuntimeExecutor, AgentRuntimeResolver, ResolvedAgent, RunManager, TaskRegistry,
    TaskScheduler,
};
use agent_storage::ConversationStore;
use anyhow::{Context, Result};
use clap::Parser;
use telegram_tool::TelegramGateway;
use agent_server::auth::TelegramAuth;

/// Start the agent platform HTTP server.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Directory containing providers/, agents/, tasks/, and tool data.
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

    /// Agent used for prompts received through Telegram.
    #[arg(long, default_value = "assistant")]
    telegram_agent: String,

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
    fn agent_catalog(&self) -> agent_server::AgentCatalog {
        agent_server::AgentCatalog::new(self.agents.definitions().map(|agent| {
            agent_server::AgentListItem {
                name: agent.name.clone(),
                description: agent.description.clone(),
                model: agent.model.clone(),
                tools: agent.tools.clone(),
            }
        }))
    }

    fn load(
        data_dir: &std::path::Path,
        telegram_gateway: Option<Arc<TelegramGateway>>,
    ) -> Result<Self> {
        let models = ModelRegistry::load(data_dir).context("loading model providers")?;

        let mut tools = ToolRegistry::new();
        alexa_tool::register_tools(&mut tools).context("registering Alexa tools")?;
        telegram_tool::register_tools(&mut tools, telegram_gateway)
            .context("registering Telegram tools")?;
        meteosource_tool::register_tools(&mut tools).context("registering Meteosource tools")?;
        google_tool::register_tools(&mut tools).context("registering Google tools")?;
        tavily_tool::register_tools(&mut tools).context("registering Tavily tools")?;
        let tools = Arc::new(tools);

        let agents =
            AgentRegistry::load_validated(data_dir.join("agents"), &models, tools.as_ref())
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

async fn require_telegram_session<B>(
    request: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<EitherBody<B>>, Error>
where
    B: MessageBody + 'static,
{
    let path = request.path();
    if path == "/" || path.starts_with("/auth/") {
        return Ok(next.call(request).await?.map_into_left_body());
    }

    let auth = request.app_data::<web::Data<TelegramAuth>>();
    let owner = auth.and_then(|auth| auth.owner_for_request(request.request()));
    let Some(owner) = owner else {
        return Ok(request.into_response(HttpResponse::Unauthorized().finish()).map_into_right_body());
    };

    if let Some(run_id) = path.strip_prefix("/runs/").and_then(|path| path.split('/').next()) {
        let owns_run = auth.is_some_and(|auth| auth.owns_run(run_id, &owner));
        if !owns_run {
            return Ok(request.into_response(HttpResponse::NotFound().finish()).map_into_right_body());
        }
    }

    Ok(next.call(request).await?.map_into_left_body())
}

#[get("/")]
async fn console_ui() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../static/index.html"))
}

#[actix_web::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Construct one gateway and share it with the bot adapter and all Telegram
    // tools. No other component is allowed to call getUpdates for this token.
    let telegram_gateway = match std::env::var("TELEGRAM_BOT_TOKEN") {
        Ok(token) => {
            let allowed = telegram::allowed_users_from_env()?;
            Some((TelegramGateway::new(&token), allowed))
        }
        Err(_) => None,
    };
    let gateway_for_tools = telegram_gateway
        .as_ref()
        .map(|(gateway, _)| gateway.clone());

    let cookie_secure = std::env::var("WEB_COOKIE_SECURE")
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let telegram_auth = Arc::new(TelegramAuth::new(
        std::env::var("TELEGRAM_BOT_USERNAME").ok(),
        cookie_secure,
    ));

    let resolver = Arc::new(
        ConfiguredAgentResolver::load(&args.data_dir, gateway_for_tools).with_context(|| {
            format!(
                "loading platform configuration from {}",
                args.data_dir.display()
            )
        })?,
    );
    let task_registry = TaskRegistry::load(
        args.data_dir.join("tasks"),
        resolver
            .agents
            .definitions()
            .map(|agent| agent.name.clone()),
    )
    .context("loading scheduled tasks")?;

    if telegram_gateway.is_some() && resolver.agents.get(&args.telegram_agent).is_none() {
        anyhow::bail!("Telegram agent '{}' is not configured", args.telegram_agent);
    }

    let agents = resolver.agent_catalog();
    let telegram_agents = resolver
        .agents
        .definitions()
        .map(|agent| agent.name.clone())
        .collect::<Vec<_>>();
    let executor = Arc::new(AgentRuntimeExecutor::new(resolver));
    let manager = RunManager::new(executor, args.max_concurrent_runs, args.max_queued_runs)
        .context("configuring run manager")?;
    let task_scheduler = TaskScheduler::start(task_registry, manager.clone());
    let conversation_store = Arc::new(
        ConversationStore::open(args.data_dir.join("conversations"))
            .context("opening conversation storage")?,
    );

    if let Some((gateway, allowed)) = telegram_gateway {
        // Subscribe before starting the poller so the first update cannot be lost.
        let updates = gateway.subscribe();
        tokio::spawn(gateway.clone().run());
        tokio::spawn(telegram::run_bot(
            gateway,
            allowed,
            args.telegram_agent.clone(),
            telegram_agents,
            manager.clone(),
            Arc::clone(&conversation_store),
            Arc::clone(&telegram_auth),
            updates,
        ));
        println!("Telegram gateway enabled (agent: {})", args.telegram_agent);
    } else {
        println!("Telegram gateway disabled (TELEGRAM_BOT_TOKEN is not set)");
    }

    let bind_address = (args.host, args.port);
    println!(
        "Agent platform listening on http://{}:{}",
        bind_address.0, bind_address.1
    );

    let server_result = HttpServer::new(move || {
        App::new()
            .wrap(middleware::from_fn(require_telegram_session))
            .app_data(web::Data::new(manager.clone()))
            .app_data(web::Data::new(agents.clone()))
            .app_data(web::Data::new(Arc::clone(&conversation_store)))
            .app_data(web::Data::from(Arc::clone(&telegram_auth)))
            .service(console_ui)
            .service(agent_server::auth::auth_api())
            .service(agent_server::conversations_api())
            .service(agent_server::agents_api())
            .service(agent_server::runs_api())
    })
    .bind(bind_address)
    .with_context(|| format!("binding HTTP server to {}:{}", args.host, args.port))?
    .run()
    .await;

    task_scheduler.shutdown().await;
    server_result.context("running HTTP server")
}
