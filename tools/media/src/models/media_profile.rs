//! A personal or family-managed person profile used for media preferences and activity.
use agent_storage::{FamilyId, UserId};
use serde::{Deserialize, Serialize};

use super::MediaProfileId;

/// A media profile belongs either to an EME user or to a family.
///
/// Personal profiles have an owner and no family. Family profiles retain the
/// existing optional EME-user assignment for family-managed households.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaProfile {
    pub id: MediaProfileId,
    #[serde(default)]
    pub family_id: Option<FamilyId>,
    #[serde(default)]
    pub owner_id: Option<UserId>,
    pub name: String,
    pub user_id: Option<UserId>,
    pub active: bool,
}

impl MediaProfile {
    pub fn new(family_id: FamilyId, name: impl Into<String>) -> Self {
        Self {
            id: MediaProfileId::new(),
            family_id: Some(family_id),
            owner_id: None,
            name: name.into(),
            user_id: None,
            active: true,
        }
    }

    /// Create a private profile owned directly by an EME user, without a family.
    pub fn new_personal(owner_id: UserId, name: impl Into<String>) -> Self {
        Self {
            id: MediaProfileId::new(),
            family_id: None,
            owner_id: Some(owner_id),
            name: name.into(),
            user_id: None,
            active: true,
        }
    }
}
