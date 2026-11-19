//! Persistence contracts and storage backends for the EME agent platform.
mod connected_calendar;
mod contracts;
mod conversation;
mod external_connection;
mod family;
mod family_invitation;
mod family_invitation_memory;
mod family_memory;
mod identity;
mod memory;
mod sqlite_conversation;
mod sqlite_execution;
mod sqlite_family;

pub use connected_calendar::{
    CalendarAccessMode, CalendarFamilyAccess, ConnectedCalendar, ConnectedCalendarId,
    ConnectedCalendarStorageError, ConnectedCalendarStore, DiscoveredCalendar,
    SqliteConnectedCalendarStore,
};
pub use contracts::{
    ConversationMessageStore, ExecutionStore, RunEventStore, RunStore, StorageError, StoredMessage, StoredRun,
    StoredRunEvent, StoredRunStatus,
};
pub use conversation::{Conversation, ConversationId, ConversationStore, ConversationStoreError};
pub use external_connection::{
    ConsumedExternalAuthorization, CredentialCipher, ExternalConnection, ExternalConnectionId,
    ExternalConnectionStatus, ExternalConnectionStorageError, ExternalConnectionStore,
    NewExternalAuthorization, NewExternalConnection, SqliteExternalConnectionStore,
};
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
pub use sqlite_conversation::{LegacyImportReport, SqliteConversationStore};
pub use sqlite_execution::SqliteExecutionStore;
pub use sqlite_family::SqliteFamilyStore;
