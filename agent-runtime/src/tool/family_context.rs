//! Trusted family membership details resolved by the host platform.

use serde::Serialize;

/// One member of the family selected for a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FamilyMemberContext {
    /// Internal identifier used by the host to resolve the display name.
    #[serde(skip)]
    pub user_id: String,
    /// Display name from the platform identity record, when available.
    pub name: Option<String>,
    /// Platform permission within the family (owner, admin, member).
    pub role: String,
    /// Family relationship (father, mother, son, daughter, etc.).
    pub relationship: String,
}

/// Trusted family information resolved by the platform before a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FamilyContext {
    pub family_id: String,
    pub family_name: String,
    /// The authenticated user's permission within this family.
    pub family_role: String,
    /// The authenticated user's relationship within this family.
    pub relationship: String,
    /// Complete active family roster.
    pub members: Vec<FamilyMemberContext>,
}
