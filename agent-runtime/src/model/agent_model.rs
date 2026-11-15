use async_trait::async_trait;
use agent_models::error::ModelError;
use crate::{ModelRequest, ModelTurn};

/// Interface used by the agent runtime to interact with a model.
#[async_trait]
pub trait AgentModel: Send + Sync {
    async fn complete(&self, request: ModelRequest) -> Result<ModelTurn, ModelError>;
}
