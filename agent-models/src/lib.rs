//! Provider-independent model contracts and integrations for the AI Platform.
//!
//! This crate defines the common conversation API used by the agent runtime,
//! including messages, requests, responses, tool definitions, token usage, and
//! errors. It also contains provider adapters and the configuration-backed
//! registry that resolves a configured model identifier into a conversation
//! client.
//!
//! Provider-specific HTTP details and wire-format data remain encapsulated inside
//! their respective adapters; callers work with the shared model types instead.
//! Agent execution, tool authorization, conversation lifecycle, and orchestration
//! are responsibilities of the runtime and higher-level crates, not this crate.

mod client;
mod error;
mod models;
mod providers;
mod registry;
mod types;

pub use client::ConversationClient;
pub use error::{ModelError, ModelFailureCategory};
pub use models::{ModelCapability, ModelConfig};
pub use registry::Registry;
pub use types::{
    ConversationMessage, ConversationRequest, ConversationResponse, ProviderReportedCost,
    TokenUsage, ToolCall, ToolDefinition,
};
