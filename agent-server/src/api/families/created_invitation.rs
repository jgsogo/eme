use agent_storage::FamilyInvitationSummary;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CreatedInvitation {
    pub invitation: FamilyInvitationSummary,
    /// Returned only by the creation operation; never included in list responses.
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lan_url: Option<String>,
}
