use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct TransferOwnershipRequest {
    pub new_owner_user_id: String,
}
