use serde_json::Value;

pub mod model;
pub mod policy;
pub mod runtime;
pub mod tool;
pub use model::AgentModel;
pub use policy::{AllowListPolicy, Authorization, PolicyEngine};
pub use runtime::{
    AgentRuntime, EventSink, NoopEventSink, RunEvent, RunEventKind, RunResult, RunStatus,
    RuntimeError,
};
pub use tool::{Tool, ToolDispatcher, ToolError, ToolRegistry};

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
#[derive(Debug, Clone)]
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
