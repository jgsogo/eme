//! Runtime model-routing contract and fixed-model adapter.

use std::sync::Arc;

use agent_models::{ModelCapability, ModelError};
use async_trait::async_trait;

use crate::{AgentModel, CancellationToken, ModelRequest, ModelTurn};

/// A model response together with the provider-qualified identity that produced it.
#[derive(Debug, Clone)]
pub struct RoutedModelTurn {
    pub model_id: String,
    pub turn: ModelTurn,
}

/// Selects and invokes a model for one agent turn.
///
/// Implementations receive the agent's requirements on every invocation. The
/// runtime deliberately knows nothing about provider configuration or credentials.
#[async_trait]
pub trait ModelRouter: Send + Sync {
    async fn complete(
        &self,
        required: &[ModelCapability],
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> Result<RoutedModelTurn, ModelError>;
}

/// Adapts the legacy single-model interface for tests and embedded callers.
pub(crate) struct FixedModelRouter(Arc<dyn AgentModel>);

impl FixedModelRouter {
    pub(crate) fn new(model: Arc<dyn AgentModel>) -> Self {
        Self(model)
    }
}

#[async_trait]
impl ModelRouter for FixedModelRouter {
    async fn complete(
        &self,
        _required: &[ModelCapability],
        request: ModelRequest,
        _cancellation: CancellationToken,
    ) -> Result<RoutedModelTurn, ModelError> {
        let turn = self.0.complete(request).await?;
        Ok(RoutedModelTurn {
            model_id: "fixed".to_owned(),
            turn,
        })
    }
}
