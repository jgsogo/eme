//! Execution engine for an individual AI agent run.
//!
//! This crate coordinates the agent conversation loop, model adaptation,
//! tool registration and dispatch, argument validation, deterministic
//! authorization, execution limits, cancellation, and runtime events.
//!
//! It executes individual runs. Managing concurrent runs and their
//! service-level lifecycle belongs to `agent-service`; provider-specific
//! integrations belong to `agent-models`; persistence and external
//! transport belong to their respective platform crates.

pub mod error;
pub mod model;
pub mod policy;
pub mod runtime;
pub mod tool;
mod types;

pub use agent_models::error::ModelError;
pub use error::{ErrorCategory, RuntimeError};
pub use model::{AgentModel, ModelAdapter};
pub use policy::{AllowListPolicy, Authorization, AuthorizationError, PolicyEngine};
pub use runtime::{
    AgentRuntime, CancellationToken, CollectingEventSink, EventSink, NoopEventSink, RunEvent,
    RunEventKind, RunFailure, RunResult, RunStatus,
};
pub use tool::{FamilyContext, Tool, ToolContext, ToolDispatcher, ToolError, ToolRegistry};
pub use types::{Agent, Message, ModelRequest, ModelTurn, ToolCall, ToolDefinition};

mod agent;
pub use agent::AgentDefinition;
mod agent_registry;
pub use agent_registry::AgentRegistry;
mod tool_definitions;
pub use tool_definitions::ToolDefinitionRegistry;
