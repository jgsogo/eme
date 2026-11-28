//! Loads configured providers, tools, and agents into runtime instances.

use std::sync::{Arc, Weak};

use agent_models::{
    ModelCatalogOverrides, ModelCapability, ModelError, ModelFailureCategory,
    Registry as ModelRegistry,
};
use agent_runtime::{
    Agent, AgentModel, AgentRegistry, AgentRuntime, AllowListPolicy, CancellationToken,
    ModelAdapter, ModelRequest, ModelRouter, ModelRoutingEvent, ModelRoutingEventSink,
    RoutedModelTurn, Tool, ToolContext, ToolDefinition, ToolDispatcher, ToolError, ToolRegistry,
};
use agent_service::{AgentRuntimeResolver, ResolvedAgent};
use agent_storage::FamilyStore;
use books_tool::BookStore;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{Value, json};
use uuid::Uuid;
use google_tool::GoogleCalendarRuntimeConfig;
use meal_planner::MealPlannerStore;
use media_tools::MediaToolServices;
use notes_tools::NotesStore;
use tasks_tool::TaskRegistry;
use telegram_tool::TelegramGateway;

use super::console_event_sink::ConsoleEventSink;

/// Resolves configured agent names into concrete runtime instances.
pub(super) struct ConfiguredAgentResolver {
    pub(super) agents: Arc<AgentRegistry>,
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
        // Load agent definitions before the tool registry so the built-in
        // delegation tool can resolve targets without depending on the service
        // queue (which would deadlock when all workers wait for child runs).
        let agents = Arc::new(
            AgentRegistry::load(data_dir.join("agents"))
                .context("loading agent definitions")?,
        );
        let tools = Arc::new_cyclic(|weak_tools| {
            tools
                .register(
                    ToolDefinition {
                        name: "delegate_agent".into(),
                        description: "Delegate a task to an explicitly authorized specialized agent and wait for its result.".into(),
                        parameters: json!({
                            "type": "object",
                            "properties": {
                                "agent": { "type": "string", "description": "Configured child agent identifier" },
                                "task": { "type": "string", "description": "Task for the child agent" },
                                "context": { "type": "string", "description": "Optional additional context explicitly shared with the child" }
                            },
                            "required": ["agent", "task"],
                            "additionalProperties": false
                        }),
                    },
                    DelegateAgentTool {
                        agents: Arc::clone(&agents),
                        models: Arc::clone(&models),
                        tools: weak_tools.clone(),
                    },
                )
                .expect("built-in delegate_agent definition is valid");
            tools
        });
        agents
            .validate_references(models.as_ref(), tools.as_ref())
            .context("validating agent model, tool, and delegation references")?;
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


/// Executes a child agent in an isolated conversation using its own tool grants.
struct DelegateAgentTool {
    agents: Arc<AgentRegistry>,
    models: Arc<ModelRegistry>,
    tools: Weak<ToolRegistry>,
}

#[async_trait]
impl Tool for DelegateAgentTool {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let target = arguments
            .get("agent")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| ToolError::Execution("delegate_agent requires a non-empty 'agent'".into()))?;
        let task = arguments
            .get("task")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| ToolError::Execution("delegate_agent requires a non-empty 'task'".into()))?;

        let parent_name = context.agent_id.as_deref().ok_or_else(|| {
            ToolError::Execution("delegation requires a trusted parent agent identity".into())
        })?;
        let parent = self.agents.get(parent_name).ok_or_else(|| {
            ToolError::Execution(format!("unknown parent agent '{parent_name}'"))
        })?;
        if !parent.allowed_delegates.iter().any(|allowed| allowed == target) {
            return Err(ToolError::Execution(format!(
                "agent '{parent_name}' is not allowed to delegate to '{target}'"
            )));
        }
        if context.delegation_depth >= parent.max_delegation_depth {
            return Err(ToolError::Execution(format!(
                "agent '{parent_name}' reached its maximum delegation depth of {}",
                parent.max_delegation_depth
            )));
        }

        let definition = self.agents.get(target).ok_or_else(|| {
            ToolError::Execution(format!("unknown child agent '{target}'"))
        })?;
        let child_agent = Agent::from(definition);
        let tools = self.tools.upgrade().ok_or_else(|| {
            ToolError::Execution("agent tool registry is no longer available".into())
        })?;
        let dispatcher: Arc<dyn ToolDispatcher> = tools;
        let policy = Arc::new(AllowListPolicy::new(child_agent.tool_names.clone()));
        let runtime = AgentRuntime::new_with_router(
            Arc::new(RegistryModelRouter::new(Arc::clone(&self.models))),
            dispatcher,
            policy,
        )
        .with_event_sink(Arc::new(ConsoleEventSink));

        // Preserve only trusted host identity and authorization context. The
        // child receives no parent transcript or conversation identifier.
        let mut child_context = context.clone();
        child_context.agent_id = Some(target.to_owned());
        child_context.delegation_depth += 1;
        child_context.conversation_id = None;
        child_context.run_id = None;

        let mut child_input = task.to_owned();
        if let Some(extra_context) = arguments
            .get("context")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            child_input.push_str("\n\nExplicit context from the parent:\n");
            child_input.push_str(extra_context);
        }

        let child_run_id = Uuid::new_v4();
        let cancellation = context.cancellation.clone().unwrap_or_default();
        let result = runtime
            .run(
                child_run_id,
                &child_agent,
                child_context,
                Vec::new(),
                child_input,
                cancellation,
            )
            .await
            .map_err(|failure| {
                ToolError::Execution(format!(
                    "child agent '{target}' failed: {}",
                    failure.error
                ))
            })?;

        Ok(json!({
            "run_id": child_run_id,
            "parent_run_id": context.run_id,
            "agent": target,
            "status": format!("{:?}", result.status).to_lowercase(),
            "output": result.response,
            "iterations": result.iterations
        }).to_string())
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
