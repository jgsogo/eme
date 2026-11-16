//! Adapter boundary between provider-independent model clients and agent execution.

mod adapter;
mod agent_model;

pub use adapter::ModelAdapter;
pub use agent_model::AgentModel;
