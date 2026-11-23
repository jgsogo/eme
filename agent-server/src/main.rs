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
#[path = "main/media_integration.rs"]
mod media_integration;
mod telegram;

use std::sync::Arc;

use actix_web::{App, HttpServer, middleware, web};
use agent_server::{LanInvitationConfig, auth::TelegramAuth};
use agent_service::{AgentRuntimeExecutor, FamilyService, RunManager};
use agent_storage::{
    FamilyInvitationStore, FamilyStore, FileUserIdentityStore, SqliteConversationStore,
    SqliteExecutionStore, SqliteFamilyStore,
};
use anyhow::{Context, Result};
use clap::Parser;
use meal_planner::{MealPlannerConfig, MealPlannerStore};
use tasks_tool::{TaskRegistry, TaskScheduler};
use telegram_tool::TelegramGateway;
use tracing::{error, info, warn};

use args::Args;
use auth_middleware::require_telegram_session;
use configured_agent_resolver::ConfiguredAgentResolver;
use console::{
    console_auth_script, console_root, console_script, console_stylesheet, conversations_page,
    desktop_header_script, families_page, favicon, integrations_page, invitation_page,
    meals_family_script, meals_page, mobile_conversations_page, mobile_families_page,
    mobile_notes_page, mobile_profile_page, mobile_root, mobile_script, mobile_stylesheet,
    mobile_tasks_page, notes_page, profile_page, recipes_page, recipes_script, school_menus_page,
    school_menus_script, shopping_list_page, shopping_list_script, tasks_page, usage_page,
    usage_script, usage_stylesheet, weekly_planner_page, weekly_planner_script,
};
use media_integration::MediaIntegration;

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

    let meal_planner_config = match args.meal_planner_database {
        Some(path) => MealPlannerConfig::new(path)?,
        None => MealPlannerConfig::from_data_dir(&args.data_dir),
    };
    let meal_planner_store =
        MealPlannerStore::open(&meal_planner_config).context("opening Meal Planner storage")?;
    let meal_planner_schema_version = meal_planner_store.schema_version()?;
    info!(
        database = %meal_planner_config.database_path().display(),
        schema_version = meal_planner_schema_version,
        "Meal Planner storage initialized"
    );

    let family_store = Arc::new(
        SqliteFamilyStore::open(args.data_dir.join("families.sqlite"))
            .context("opening family and invitation storage")?,
    );
    let family_service = Arc::new(FamilyService::new(
        Arc::clone(&family_store) as Arc<dyn FamilyStore>,
        Arc::clone(&family_store) as Arc<dyn FamilyInvitationStore>,
    ));
    let google_oauth_state = agent_server::GoogleOAuthState::from_env(&args.data_dir)
        .map_err(anyhow::Error::msg)
        .context("configuring Google OAuth integration")?;
    let google_runtime = google_oauth_state.calendar_runtime(Arc::clone(&family_service));
    let media_integration = MediaIntegration::from_env(
        &args.data_dir,
        Arc::clone(&family_store) as Arc<dyn FamilyStore>,
    )
    .context("configuring media integrations")?;
    let media_api_state = media_integration.api.clone();

    let resolver = Arc::new(
        ConfiguredAgentResolver::load(
            &args.data_dir,
            gateway_for_tools,
            task_registry.clone(),
            meal_planner_store.clone(),
            Arc::clone(&family_store) as Arc<dyn FamilyStore>,
            google_runtime,
            media_integration.tools,
        )
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
    let notes_store = resolver.notes.clone();
    let telegram_agents = resolver
        .agents
        .definitions()
        .map(|agent| agent.name.clone())
        .collect::<Vec<_>>();
    let executor = Arc::new(AgentRuntimeExecutor::new(resolver));
    let identity_store = Arc::new(
        FileUserIdentityStore::open(args.data_dir.join("users.json"))
            .context("opening persistent user identity storage")?,
    );
    let conversation_database = args
        .conversation_database
        .clone()
        .unwrap_or_else(|| args.data_dir.join("agent-storage.sqlite"));
    let conversation_store = Arc::new(
        SqliteConversationStore::open(&conversation_database)
            .context("opening SQLite conversation storage")?,
    );
    let import_report = conversation_store
        .import_legacy(args.data_dir.join("conversations"))
        .context("importing legacy conversations into SQLite")?;
    info!(
        database = %conversation_database.display(),
        imported = import_report.imported,
        already_present = import_report.already_present,
        active_mappings_imported = import_report.active_mappings_imported,
        "conversation storage initialized"
    );
    if !import_report.invalid_documents.is_empty()
        || !import_report.invalid_active_entries.is_empty()
    {
        anyhow::bail!(
            "legacy conversation import found {} invalid documents and {} invalid active mappings; fix the legacy data and restart (legacy files were preserved)",
            import_report.invalid_documents.len(),
            import_report.invalid_active_entries.len()
        );
    }
    let execution_store = Arc::new(
        SqliteExecutionStore::open(&conversation_database)
            .context("opening SQLite execution history storage")?,
    );
    let manager = RunManager::new(executor, args.max_concurrent_runs, args.max_queued_runs)
        .context("configuring run manager")?
        .with_storage(execution_store);
    let task_scheduler = TaskScheduler::start(task_registry.clone(), manager.clone());
    info!("task scheduler started");
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

    let lan_invitation_config = LanInvitationConfig::from_env()
        .map_err(anyhow::Error::msg)
        .context("validating LAN_BASE_URL")?;
    if lan_invitation_config.is_some() {
        info!("LAN invitation links enabled");
    } else {
        error!(
            "LAN_BASE_URL is not set; LAN family invitation links are disabled. \
             Set LAN_BASE_URL to the server's private-network URL (for example \
             http://192.168.1.20:8080) and restart agent-server."
        );
    }

    let bind_address = (args.host, args.port);
    info!(address = %format!("http://{}:{}", bind_address.0, bind_address.1), "HTTP server listening");

    let server_result = HttpServer::new(move || {
        let mut app = App::new()
            .wrap(middleware::Logger::default())
            .wrap(middleware::from_fn(require_telegram_session))
            .app_data(web::Data::new(manager.clone()))
            .app_data(web::Data::new(agents.clone()))
            .app_data(web::Data::new(Arc::clone(&conversation_store)))
            .app_data(web::Data::new(Arc::clone(&identity_store)))
            .app_data(web::Data::new(Arc::clone(&family_service)))
            .app_data(web::Data::new(notes_store.clone()))
            .app_data(web::Data::new(task_registry.clone()))
            .app_data(web::Data::new(meal_planner_store.clone()))
            .app_data(web::Data::new(google_oauth_state.clone()))
            .app_data(web::Data::new(media_api_state.clone()))
            .app_data(web::Data::from(Arc::clone(&telegram_auth)));
        if let Some(config) = lan_invitation_config.clone() {
            app = app.app_data(web::Data::new(config));
        }
        app.service(console_root)
            .service(mobile_root)
            .service(mobile_conversations_page)
            .service(mobile_families_page)
            .service(mobile_notes_page)
            .service(mobile_profile_page)
            .service(mobile_tasks_page)
            .service(conversations_page)
            .service(usage_page)
            .service(usage_script)
            .service(usage_stylesheet)
            .service(meals_page)
            .service(families_page)
            .service(notes_page)
            .service(profile_page)
            .service(integrations_page)
            .service(tasks_page)
            .service(recipes_page)
            .service(recipes_script)
            .service(school_menus_page)
            .service(meals_family_script)
            .service(school_menus_script)
            .service(weekly_planner_page)
            .service(weekly_planner_script)
            .service(shopping_list_page)
            .service(shopping_list_script)
            .service(console_stylesheet)
            .service(desktop_header_script)
            .service(console_auth_script)
            .service(console_script)
            .service(mobile_stylesheet)
            .service(mobile_script)
            .service(favicon)
            .service(invitation_page)
            .service(agent_server::auth::auth_api())
            .service(agent_server::google_integrations_api())
            .service(agent_server::media_api())
            .service(agent_server::conversations_api())
            .service(agent_server::families_api())
            .service(agent_server::notes_api())
            // Register the more specific scope before /meal-planner; otherwise
            // the broader scope captures school-menu requests and returns 404.
            .service(agent_server::school_menus_api())
            .service(agent_server::meal_planner_api())
            .service(agent_server::profile_api())
            .service(agent_server::tasks_api())
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
