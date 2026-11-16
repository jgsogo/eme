//! Trusted family membership details resolved by the host platform.

/// Trusted family membership information resolved by the platform before a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyContext {
    pub family_id: String,
    pub family_name: String,
    pub family_role: String,
    pub relationship: String,
}
