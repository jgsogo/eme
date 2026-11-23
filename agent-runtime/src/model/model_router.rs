//! Runtime model-routing contract, lifecycle events, and fixed-model adapter.

use std::sync::Arc;

use agent_models::{ModelCapability, ModelError, ModelFailureCategory};
use async_trait::async_trait;

use crate::{AgentModel, CancellationToken, ModelRequest, ModelTurn};

/// Events emitted while a router evaluates compatible model candidates.
#[derive(Debug, Clone)]
pub enum ModelRoutingEvent {
    CandidatesDiscovered {
        model_ids: Vec<String>,
    },
    CandidateAttemptStarted {
        model_id: String,
    },
    CandidateAttemptFailed {
        model_id: String,
        category: ModelFailureCategory,
    },
    Fallback {
        from_model_id: String,
        to_model_id: String,
        category: ModelFailureCategory,
    },
    ModelSelected {
        model_id: String,
    },
    RoutingFailed {
        attempted_candidates: usize,
    },
}

/// Receives routing lifecycle events without exposing provider error payloads.
pub trait ModelRoutingEventSink: Send + Sync {
    fn emit(&self, event: ModelRoutingEvent);
}

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
        events: Arc<dyn ModelRoutingEventSink>,
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
        events: Arc<dyn ModelRoutingEventSink>,
    ) -> Result<RoutedModelTurn, ModelError> {
        let model_id = "fixed".to_owned();
        events.emit(ModelRoutingEvent::CandidatesDiscovered {
            model_ids: vec![model_id.clone()],
        });
        events.emit(ModelRoutingEvent::CandidateAttemptStarted {
            model_id: model_id.clone(),
        });
        match self.0.complete(request).await {
            Ok(turn) => {
                events.emit(ModelRoutingEvent::ModelSelected {
                    model_id: model_id.clone(),
                });
                Ok(RoutedModelTurn { model_id, turn })
            }
            Err(error) => {
                events.emit(ModelRoutingEvent::CandidateAttemptFailed {
                    model_id,
                    category: error.failure_category(),
                });
                events.emit(ModelRoutingEvent::RoutingFailed {
                    attempted_candidates: 1,
                });
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use agent_models::ModelError;
    use async_trait::async_trait;

    use super::{FixedModelRouter, ModelRouter, ModelRoutingEvent, ModelRoutingEventSink};
    use crate::{AgentModel, CancellationToken, ModelRequest, ModelTurn};

    struct CompletedModel;

    #[async_trait]
    impl AgentModel for CompletedModel {
        async fn complete(&self, _request: ModelRequest) -> Result<ModelTurn, ModelError> {
            Ok(ModelTurn {
                usage: None,
                content: Some("done".into()),
                tool_calls: Vec::new(),
            })
        }
    }

    #[derive(Default)]
    struct RecordingSink(Mutex<Vec<ModelRoutingEvent>>);

    impl ModelRoutingEventSink for RecordingSink {
        fn emit(&self, event: ModelRoutingEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[tokio::test]
    async fn fixed_router_emits_candidate_attempt_and_selection_events() {
        let sink = Arc::new(RecordingSink::default());
        let router = FixedModelRouter::new(Arc::new(CompletedModel));

        let routed = router
            .complete(
                &[],
                ModelRequest {
                    system_prompt: None,
                    messages: Vec::new(),
                    tools: Vec::new(),
                },
                CancellationToken::new(),
                sink.clone(),
            )
            .await
            .unwrap();

        assert_eq!(routed.model_id, "fixed");
        let events = sink.0.lock().unwrap();
        assert!(matches!(
            events.first(),
            Some(ModelRoutingEvent::CandidatesDiscovered { model_ids })
                if model_ids == &vec!["fixed".to_owned()]
        ));
        assert!(matches!(
            events.get(1),
            Some(ModelRoutingEvent::CandidateAttemptStarted { model_id })
                if model_id == "fixed"
        ));
        assert!(matches!(
            events.get(2),
            Some(ModelRoutingEvent::ModelSelected { model_id })
                if model_id == "fixed"
        ));
    }
}
