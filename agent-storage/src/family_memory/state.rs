//! Private family and membership maps.

use crate::{Family, FamilyId, FamilyMember, UserId};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct State {
    pub(super) families: HashMap<FamilyId, Family>,
    pub(super) members: HashMap<(FamilyId, UserId), FamilyMember>,
}
