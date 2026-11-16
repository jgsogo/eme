//! Executable entry point for the agent platform HTTP service.

#[path = "main/args.rs"]
mod args;
#[path = "main/auth_middleware.rs"]
mod auth_middleware;
#[path = "main/configured_agent_resolver.rs"]
mod configured_agent_resolver;
#[path = "main/console.rs"]
mod console;
#[path = "main/console_event_sink.rs"]
mod console_event_sink;
mod telegram;

use std::sync::Arc;

use actix_web::{App, HttpServer, middleware, web};
use agent_server::auth::TelegramAuth;
use agent_service::{AgentRuntimeExecutor, FamilyService, RunManager, TaskRegistry, TaskScheduler};
use agent_storage::{
    ConversationStore, FamilyInvitationStore, FamilyStore, FileUserIdentityStore, SqliteFamilyStore,
};
use anyhow::{Context, Result};
use clap::Parser;
use telegram_tool::TelegramGateway;
use tracing::{info, warn};

use args::Args;
use auth_middleware::require_telegram_session;
use configured_agent_resolver::ConfiguredAgentResolver;
use console::console_ui;

#[actix_web::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    info!(data_dir = %args.data_dir.display(), host = %args.host, port = args.port,
        max_concurrent_runs = args.max_concurrent_runs, max_queued_runs = args.max_queued_runs,
        "starting agent platform");

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

    let task_registry =
        TaskRegistry::load(args.data_dir.join("tasks")).context("loading scheduled tasks")?;

    let resolver = Arc::new(
        ConfiguredAgentResolver::load(&args.data_dir, gateway_for_tools, task_registry.clone())
            .with_context(|| {
                format!(
                    "loading platform configuration from {}",
                    args.data_dir.display()
                )
            })?,
    );
    info!(
        agents = resolver.agents.definitions().count(),
        "agent definitions loaded"
    );

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
    info!("task scheduler started");
    let identity_store = Arc::new(
        FileUserIdentityStore::open(args.data_dir.join("users.json"))
            .context("opening persistent user identity storage")?,
    );
    let conversation_store = Arc::new(
        ConversationStore::open(args.data_dir.join("conversations"))
            .context("opening conversation storage")?,
    );
    let family_store = Arc::new(
        SqliteFamilyStore::open(args.data_dir.join("families.sqlite"))
            .context("opening family and invitation storage")?,
    );
    let family_service = Arc::new(FamilyService::new(
        Arc::clone(&family_store) as Arc<dyn FamilyStore>,
        Arc::clone(&family_store) as Arc<dyn FamilyInvitationStore>,
    ));

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
            Arc::clone(&identity_store),
            Arc::clone(&family_store),
            Arc::clone(&family_service),
            Arc::clone(&telegram_auth),
            updates,
        ));
        info!(agent = %args.telegram_agent, "Telegram gateway enabled");
    } else {
        info!("Telegram gateway disabled (TELEGRAM_BOT_TOKEN is not set)");
    }

    let bind_address = (args.host, args.port);
    info!(address = %format!("http://{}:{}", bind_address.0, bind_address.1), "HTTP server listening");

    let server_result = HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .wrap(middleware::from_fn(require_telegram_session))
            .app_data(web::Data::new(manager.clone()))
            .app_data(web::Data::new(agents.clone()))
            .app_data(web::Data::new(Arc::clone(&conversation_store)))
            .app_data(web::Data::new(Arc::clone(&identity_store)))
            .app_data(web::Data::new(Arc::clone(&family_service)))
            .app_data(web::Data::from(Arc::clone(&telegram_auth)))
            .service(console_ui)
            .service(agent_server::auth::auth_api())
            .service(agent_server::conversations_api())
            .service(agent_server::families_api())
            .service(agent_server::invitations_api())
            .service(agent_server::agents_api())
            .service(agent_server::runs_api())
    })
    .bind(bind_address)
    .with_context(|| format!("binding HTTP server to {}:{}", args.host, args.port))?
    .run()
    .await;

    task_scheduler.shutdown().await;
    if let Err(error) = &server_result {
        warn!(%error, "HTTP server stopped with an error");
    }
    server_result.context("running HTTP server")
}
