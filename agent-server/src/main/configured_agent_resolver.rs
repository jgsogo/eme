//! Loads configured providers, tools, and agents into runtime instances.

use std::sync::Arc;

use agent_models::{
    ModelCatalogOverrides, ModelCapability, ModelError, ModelFailureCategory,
    Registry as ModelRegistry,
};
use agent_runtime::{
    Agent, AgentModel, AgentRegistry, AgentRuntime, AllowListPolicy, CancellationToken,
    ModelAdapter, ModelRequest, ModelRouter, ModelRoutingEvent, ModelRoutingEventSink,
    RoutedModelTurn, ToolDispatcher, ToolRegistry,
};
use agent_service::{AgentRuntimeResolver, ResolvedAgent};
use agent_storage::FamilyStore;
use books_tool::BookStore;
use anyhow::{Context, Result};
use async_trait::async_trait;
use google_tool::GoogleCalendarRuntimeConfig;
use meal_planner::MealPlannerStore;
use media_tools::MediaToolServices;
use notes_tools::NotesStore;
use tasks_tool::TaskRegistry;
use telegram_tool::TelegramGateway;

use super::console_event_sink::ConsoleEventSink;

/// Resolves configured agent names into concrete runtime instances.
pub(super) struct ConfiguredAgentResolver {
    pub(super) agents: AgentRegistry,
    models: Arc<ModelRegistry>,
    tools: Arc<ToolRegistry>,
    pub(super) notes: NotesStore,
    pub(super) books: BookStore,
}

impl ConfiguredAgentResolver {
    pub(super) fn provider_catalog(&self) -> Arc<ModelRegistry> {
        Arc::clone(&self.models)
    }

    pub(super) fn tool_catalog(&self) -> Vec<agent_runtime::ToolDefinition> {
        self.tools.definitions()
    }

    pub(super) fn agent_catalog(&self) -> agent_server::AgentCatalog {
        agent_server::AgentCatalog::new(self.agents.definitions().map(|agent| {
            agent_server::AgentListItem {
                name: agent.name.clone(),
                description: agent.description.clone(),
                capabilities: agent.capabilities.clone(),
                tools: agent.tools.clone(),
            }
        }))
    }

    pub(super) fn load(
        data_dir: &std::path::Path,
        telegram_gateway: Option<Arc<TelegramGateway>>,
        task_registry: TaskRegistry,
        meal_planner_store: MealPlannerStore,
        family_store: Arc<dyn FamilyStore>,
        google_runtime: Option<GoogleCalendarRuntimeConfig>,
        media_services: MediaToolServices,
        model_catalog_overrides: ModelCatalogOverrides,
    ) -> Result<Self> {
        let model_registry =
            ModelRegistry::load(data_dir).context("loading model providers")?;
        model_registry.apply_overrides(model_catalog_overrides);
        let models = Arc::new(model_registry);

        let mut tools = ToolRegistry::new();
        let books = BookStore::open(data_dir.join("books.sqlite")).context("opening personal book collection")?;
        books_tool::register_tools(&mut tools, books.clone()).context("registering book collection tools")?;
        alexa_tool::register_tools(&mut tools).context("registering Alexa tools")?;
        telegram_tool::register_tools(&mut tools, telegram_gateway)
            .context("registering Telegram tools")?;
        meteosource_tool::register_tools(&mut tools).context("registering Meteosource tools")?;
        rabel_tool::register_tools(&mut tools).context("registering RABEL tools")?;
        google_tool::register_tools(&mut tools, google_runtime)
            .context("registering Google tools")?;
        tavily_tool::register_tools(&mut tools).context("registering Tavily tools")?;
        common_tools::register_tools(&mut tools).context("registering common tools")?;
        mercadona_tool::register_tools(&mut tools).context("registering Mercadona tools")?;
        meal_planner::register_tools(&mut tools, meal_planner_store)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
            .context("registering Meal Planner tools")?;
        tasks_tool::register_tools(&mut tools, task_registry.clone())
            .context("registering task tools")?;
        let notes =
            notes_tools::register_tools(&mut tools, family_store, data_dir.join("notes.sqlite"))
                .map_err(|error| anyhow::anyhow!(error.to_string()))
                .context("registering notes tools")?;
        media_tools::register_media_tools(&mut tools, media_services)
            .map_err(|error| anyhow::anyhow!(error))
            .context("registering media tools")?;
        home_assistant_tool::register_tools(&mut tools)
            .context("registering Home Assistant tools")?;
        let tools = Arc::new(tools);

        let agents =
            AgentRegistry::load_validated(data_dir.join("agents"), models.as_ref(), tools.as_ref())
                .context("loading and validating agent definitions")?;
        task_registry
            .validate_agents(agents.definitions().map(|agent| agent.name.clone()))
            .context("validating task agent references")?;

        Ok(Self {
            agents,
            models,
            tools,
            notes,
            books,
        })
    }
}

impl AgentRuntimeResolver for ConfiguredAgentResolver {
    fn resolve(&self, agent_name: &str) -> Result<ResolvedAgent, String> {
        let definition = self
            .agents
            .get(agent_name)
            .ok_or_else(|| format!("unknown agent '{agent_name}'"))?;

        let agent = Agent::from(definition);
        let model_router = Arc::new(RegistryModelRouter::new(Arc::clone(&self.models)));
        let tools: Arc<dyn ToolDispatcher> = self.tools.clone();
        let policy = Arc::new(AllowListPolicy::new(agent.tool_names.clone()));
        let runtime = Arc::new(
            AgentRuntime::new_with_router(model_router, tools, policy)
                .with_event_sink(Arc::new(ConsoleEventSink)),
        );

        Ok(ResolvedAgent { agent, runtime })
    }
}

struct RegistryModelRouter {
    registry: Arc<ModelRegistry>,
}

impl RegistryModelRouter {
    fn new(registry: Arc<ModelRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ModelRouter for RegistryModelRouter {
    async fn complete(
        &self,
        required: &[ModelCapability],
        request: ModelRequest,
        cancellation: CancellationToken,
        events: Arc<dyn ModelRoutingEventSink>,
    ) -> Result<RoutedModelTurn, ModelError> {
        let candidates = self.registry.compatible_models(required);
        events.emit(ModelRoutingEvent::CandidatesDiscovered {
            model_ids: candidates.clone(),
        });
        if candidates.is_empty() {
            events.emit(ModelRoutingEvent::RoutingFailed {
                attempted_candidates: 0,
            });
            return Err(ModelError::UnsupportedModel(format!(
                "no enabled model satisfies required capabilities {required:?}"
            )));
        }

        let mut failures = Vec::new();

        for (index, model_id) in candidates.iter().enumerate() {
            if cancellation.is_cancelled() {
                return Err(ModelError::Cancelled);
            }

            let model_id = model_id.clone();
            events.emit(ModelRoutingEvent::CandidateAttemptStarted {
                model_id: model_id.clone(),
            });
            let attempt = match self.registry.resolve_model(&model_id) {
                Ok(client) => ModelAdapter::new(client).complete(request.clone()).await,
                Err(error) => Err(error),
            };

            match attempt {
                Ok(turn) => {
                    events.emit(ModelRoutingEvent::ModelSelected {
                        model_id: model_id.clone(),
                    });
                    return Ok(RoutedModelTurn { model_id, turn });
                }
                Err(error) => {
                    let category = error.failure_category();
                    events.emit(ModelRoutingEvent::CandidateAttemptFailed {
                        model_id: model_id.clone(),
                        category,
                    });
                    if category == ModelFailureCategory::Permanent {
                        events.emit(ModelRoutingEvent::RoutingFailed {
                            attempted_candidates: index + 1,
                        });
                        return Err(error);
                    }

                    failures.push(format!("{model_id} [{category:?}]: {error}"));
                    if let Some(next_model_id) = candidates.get(index + 1) {
                        events.emit(ModelRoutingEvent::Fallback {
                            from_model_id: model_id,
                            to_model_id: next_model_id.clone(),
                            category,
                        });
                    }
                }
            }
        }

        if cancellation.is_cancelled() {
            return Err(ModelError::Cancelled);
        }

        events.emit(ModelRoutingEvent::RoutingFailed {
            attempted_candidates: candidates.len(),
        });
        Err(ModelError::AllCandidatesFailed(failures.join("; ")))
    }
}
