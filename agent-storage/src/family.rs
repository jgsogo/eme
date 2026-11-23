//! Family records, membership rules, and persistence contract.

#[path = "family/family.rs"]
mod family_record;
mod family_id;
mod family_member;
mod family_relationship;
mod family_role;
mod family_storage_error;
mod family_store;

pub use family_record::Family;
pub use family_id::FamilyId;
pub use family_member::FamilyMember;
pub use family_relationship::FamilyRelationship;
pub use family_role::FamilyRole;
pub use family_storage_error::FamilyStorageError;
pub use family_store::FamilyStore;
