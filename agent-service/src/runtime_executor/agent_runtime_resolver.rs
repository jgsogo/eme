//! The contract for resolving configured agent names into runtime instances.

use super::ResolvedAgent;

/// Resolves an agent name to its definition and configured runtime.
///
/// The resolver is the composition boundary for agent definitions, model
/// clients, tool registries, and authorization policies. Its implementation
/// belongs in the application/bootstrap layer, not in the HTTP transport.
pub trait AgentRuntimeResolver: Send + Sync + 'static {
    fn resolve(&self, agent_name: &str) -> Result<ResolvedAgent, String>;
}
