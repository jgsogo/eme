//! Family identifier typeDescribes a member's relationship

use serde::{Deserialize, Serialize};

/// Describes a member's relationship within one family, not their permissions.
/// Add variants as the product vocabulary evolves; do not infer tree constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FamilyRelationship {
    Father,
    Mother,
    Son,
    Daughter,
    Grandfather,
    Grandmother,
    Partner,
    Other,
}
