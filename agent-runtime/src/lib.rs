//! Execution engine for an individual AI agent run.
//!
//! This crate will coordinate the agent conversation loop, model adaptation,
//! tool registration and dispatch, argument validation, deterministic
//! authorization, execution limits, cancellation, and runtime events.
//!
//! It will execute individual runs. Managing concurrent runs and their
//! service-level lifecycle will belong to `agent-service`; provider-specific
//! integrations will belong to `agent-models`; persistence and external
//! transport will belong to their respective platform crates.

use serde::Deserialize;
use serde_json::Value;

pub mod error;
pub mod model;
pub mod policy;
pub mod runtime;
pub mod tool;
pub use agent_models::error::ModelError;
pub use error::{ErrorCategory, RuntimeError};
pub use model::{AgentModel, ModelAdapter};
pub use policy::{AllowListPolicy, Authorization, AuthorizationError, PolicyEngine};
pub use runtime::{
    AgentRuntime, CancellationToken, CollectingEventSink, EventSink, NoopEventSink, RunEvent,
    RunEventKind, RunFailure, RunResult, RunStatus,
};
pub use tool::{Tool, ToolDispatcher, ToolError, ToolRegistry};
mod tool_definitions;
pub use tool_definitions::ToolDefinitionRegistry;
mod agent;

pub use agent::AgentDefinition;
mod agent_registry;

pub use agent_registry::AgentRegistry;

/// Configuration for an agent.
#[derive(Debug, Clone)]
pub struct Agent {
    pub id: String,
    pub system_prompt: Option<String>,
    /// Names of the tools available to this agent.
    pub tool_names: Vec<String>,
    pub max_iterations: usize,
}

/// Description of a tool exposed to the model.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema describing the tool arguments.
    pub parameters: Value,
}

/// A tool invocation requested by the model.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    pub provider_metadata: Option<Value>,
}

/// A message in the conversation.
#[derive(Debug, Clone)]
pub enum Message {
    User {
        content: String,
    },
    Assistant {
        content: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
    Tool {
        tool_call_id: String,
        name: String,
        content: String,
        is_error: bool,
        provider_metadata: Option<Value>,
    },
}

/// Input sent to the model.
#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
}

/// Response received from the model.
#[derive(Debug, Clone)]
pub struct ModelTurn {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}
