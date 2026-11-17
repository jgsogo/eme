//! Loads configured providers, tools, and agents into runtime instances.

use std::sync::Arc;

use agent_models::Registry as ModelRegistry;
use agent_runtime::{
    Agent, AgentRegistry, AgentRuntime, AllowListPolicy, ModelAdapter, ToolDispatcher, ToolRegistry,
};
use agent_service::{AgentRuntimeResolver, ResolvedAgent};
use agent_storage::FamilyStore;
use anyhow::{Context, Result};
use notes_tools::NotesStore;
use tasks_tool::TaskRegistry;
use telegram_tool::TelegramGateway;
use google_tool::GoogleCalendarRuntimeConfig;

use super::console_event_sink::ConsoleEventSink;

/// Resolves configured agent names into concrete runtime instances.
pub(super) struct ConfiguredAgentResolver {
    pub(super) agents: AgentRegistry,
    models: ModelRegistry,
    tools: Arc<ToolRegistry>,
    pub(super) notes: NotesStore,
}

impl ConfiguredAgentResolver {
    pub(super) fn agent_catalog(&self) -> agent_server::AgentCatalog {
        agent_server::AgentCatalog::new(self.agents.definitions().map(|agent| {
            agent_server::AgentListItem {
                name: agent.name.clone(),
                description: agent.description.clone(),
                model: agent.model.clone(),
                tools: agent.tools.clone(),
            }
        }))
    }

    pub(super) fn load(
        data_dir: &std::path::Path,
        telegram_gateway: Option<Arc<TelegramGateway>>,
        task_registry: TaskRegistry,
        family_store: Arc<dyn FamilyStore>,
        google_runtime: Option<GoogleCalendarRuntimeConfig>,
    ) -> Result<Self> {
        let models = ModelRegistry::load(data_dir).context("loading model providers")?;

        let mut tools = ToolRegistry::new();
        alexa_tool::register_tools(&mut tools).context("registering Alexa tools")?;
        telegram_tool::register_tools(&mut tools, telegram_gateway)
            .context("registering Telegram tools")?;
        meteosource_tool::register_tools(&mut tools).context("registering Meteosource tools")?;
        google_tool::register_tools(&mut tools, google_runtime).context("registering Google tools")?;
        tavily_tool::register_tools(&mut tools).context("registering Tavily tools")?;
        common_tools::register_tools(&mut tools).context("registering common tools")?;
        tasks_tool::register_tools(&mut tools, task_registry.clone())
            .context("registering task tools")?;
        let notes =
            notes_tools::register_tools(&mut tools, family_store, data_dir.join("notes.sqlite"))
                .map_err(|error| anyhow::anyhow!(error.to_string()))
                .context("registering notes tools")?;
        let tools = Arc::new(tools);

        let agents =
            AgentRegistry::load_validated(data_dir.join("agents"), &models, tools.as_ref())
                .context("loading and validating agent definitions")?;
        task_registry
            .validate_agents(agents.definitions().map(|agent| agent.name.clone()))
            .context("validating task agent references")?;

        Ok(Self {
            agents,
            models,
            tools,
            notes,
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
        let runtime = Arc::new(
            AgentRuntime::new(model, tools, policy).with_event_sink(Arc::new(ConsoleEventSink)),
        );

        Ok(ResolvedAgent { agent, runtime })
    }
}
