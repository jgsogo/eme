//! Explicit participant selection and policy for one recommendation request.
use agent_storage::FamilyId;
use serde::{Deserialize, Serialize};

use super::{MediaProfileId, RecommendationCriteria, WatchedPolicy};

/// Context is data, not an authorization grant; the server must validate family ownership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecommendationContext {
    pub family_id: FamilyId,
    pub participants: Vec<MediaProfileId>,
    pub criteria: RecommendationCriteria,
    /// Per-request override; `None` uses the global media configuration.
    pub watched_policy: Option<WatchedPolicy>,
}
