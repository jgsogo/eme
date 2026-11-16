use chrono::{DateTime,Utc};
use agent_storage::InvitationTargetIdentity;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreateInvitationRequest {
    pub expires_at: DateTime<Utc>,
    #[serde(default)]
    pub target_identity: Option<InvitationTargetIdentity>,
}
