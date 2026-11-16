//! Private family and membership maps.

use std::collections::HashMap;
use crate::{Family, FamilyId, FamilyMember, UserId};

#[derive(Default)]
pub(super) struct State {
    pub(super) families: HashMap<FamilyId, Family>,
    pub(super) members: HashMap<(FamilyId, UserId), FamilyMember>,
}
