//! Runtime engine for executing individual AI agents.
//!
//! The crate owns the agent execution loop: it prepares model requests, processes
//! model turns, dispatches tool calls, enforces authorization policies, and
//! records lifecycle events. It also defines the contracts that connect model
//! adapters, tools, and the host application.
//!
//! The runtime deliberately does not own provider-specific APIs, durable
//! conversation storage, scheduling, or service-level orchestration. Those
//! concerns belong to the model, persistence, and service crates respectively;
//! this crate focuses on executing one agent run with trusted host context.

mod agent;
mod agent_registry;
mod error;
mod model;
mod policy;
mod runtime;
mod tool;
mod tool_definitions;
mod types;

pub use agent::AgentDefinition;
pub use agent_registry::AgentRegistry;
pub use agent_models::ModelError;
pub use error::{ErrorCategory, RuntimeError};
pub use model::{AgentModel, ModelAdapter};
pub use policy::{AllowListPolicy, Authorization, AuthorizationError, PolicyEngine};
pub use runtime::{
    AgentRuntime, CancellationToken, CollectingEventSink, EventSink, NoopEventSink, RunEvent,
    RunEventKind, RunFailure, RunResult, RunStatus,
};
pub use tool::{
    FamilyContext, Tool, ToolContext, ToolDispatcher, ToolError, ToolRegistry,
};
pub use tool_definitions::ToolDefinitionRegistry;
pub use types::{Agent, Message, ModelRequest, ModelTurn, ToolCall, ToolDefinition};
