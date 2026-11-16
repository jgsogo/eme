//! Adapters between agent runtime execution and service-level run contracts.

mod agent_runtime_executor;
mod agent_runtime_resolver;
mod resolved_agent;
#[cfg(test)]
mod tests;

pub use agent_runtime_executor::AgentRuntimeExecutor;
pub use agent_runtime_resolver::AgentRuntimeResolver;
pub use resolved_agent::ResolvedAgent;
