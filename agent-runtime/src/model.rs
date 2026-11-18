//! Adapter boundary between provider-independent model clients and agent execution.

mod adapter;
mod agent_model;
mod model_router;

pub use adapter::ModelAdapter;
pub use agent_model::AgentModel;
pub(crate) use model_router::FixedModelRouter;
pub use model_router::{ModelRouter, ModelRoutingEvent, ModelRoutingEventSink, RoutedModelTurn};
