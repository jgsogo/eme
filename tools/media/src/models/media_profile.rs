//! A family-managed person profile used for media preferences and activity.
use agent_storage::{FamilyId, UserId};
use serde::{Deserialize, Serialize};

use super::MediaProfileId;

/// A media profile may exist without an EME login.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaProfile {
    pub id: MediaProfileId,
    pub family_id: FamilyId,
    pub name: String,
    pub user_id: Option<UserId>,
    pub active: bool,
}

impl MediaProfile {
    pub fn new(family_id: FamilyId, name: impl Into<String>) -> Self {
        Self {
            id: MediaProfileId::new(),
            family_id,
            name: name.into(),
            user_id: None,
            active: true,
        }
    }
}
