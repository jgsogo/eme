//! Private invitation and token-index maps.

use std::collections::HashMap;
use uuid::Uuid;
use crate::FamilyInvitation;

#[derive(Default)]
pub(super) struct State {
    pub(super) invitations: HashMap<Uuid, FamilyInvitation>,
    pub(super) token_index: HashMap<String, Uuid>,
}
