//! Persistence contracts and storage backends for the EME agent platform.
//!
//! This crate defines the durable records and storage interfaces used by the
//! platform to retain conversations, agent runs, ordered run events, messages,
//! user identities, families, and family invitations. It provides in-memory,
//! JSON-file, and SQLite-backed implementations where appropriate, while
//! keeping the domain contracts independent of a particular service or API.
//!
//! The crate owns persistence semantics: record identity, ordering, uniqueness,
//! ownership checks performed by storage operations, and backend-specific
//! atomicity. It does not decide when an agent run should be persisted, execute
//! agents or tools, expose HTTP endpoints, or implement application-level
//! authorization. Those responsibilities remain with the service, runtime,
//! server, and policy layers that compose these storage primitives.

mod contracts;
mod conversation;
mod family;
mod family_invitation;
mod family_invitation_memory;
mod family_memory;
mod identity;
mod memory;
mod sqlite_family;

pub use contracts::{
    ConversationMessageStore, RunEventStore, RunStore, StorageError, StoredMessage, StoredRun,
    StoredRunEvent, StoredRunStatus,
};
pub use conversation::{Conversation, ConversationId, ConversationStore, ConversationStoreError};
pub use family::{
    Family, FamilyId, FamilyMember, FamilyRelationship, FamilyRole, FamilyStorageError, FamilyStore,
};
pub use family_invitation::{
    FamilyInvitation, FamilyInvitationStore, FamilyInvitationSummary, InvitationStorageError,
    InvitationTargetIdentity, InvitationToken,
};
pub use family_invitation_memory::InMemoryFamilyInvitationStore;
pub use family_memory::InMemoryFamilyStore;
pub use identity::{
    FileUserIdentityStore, IdentityStoreError, InMemoryUserIdentityStore, User, UserId,
    UserIdentity, UserIdentityStore,
};
pub use memory::InMemoryStorage;
pub use sqlite_family::SqliteFamilyStore;
