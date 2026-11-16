//! Private invitation and token-index maps.

use crate::FamilyInvitation;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Default)]
pub(super) struct State {
    pub(super) invitations: HashMap<Uuid, FamilyInvitation>,
    pub(super) token_index: HashMap<String, Uuid>,
}
