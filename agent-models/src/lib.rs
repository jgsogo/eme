//! Provider-independent model contracts, provider adapters, and model resolution.

mod client;
mod error;
mod models;
mod providers;
mod registry;
mod types;

pub use client::ConversationClient;
pub use error::ModelError;
pub use models::{ModelCapability, ModelConfig};
pub use registry::Registry;
pub use types::{
    ConversationMessage, ConversationRequest, ConversationResponse, TokenUsage, ToolCall,
    ToolDefinition,
};
