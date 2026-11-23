//! Deterministic policy for excluding titles already watched by participants.
use serde::{Deserialize, Serialize};

/// How a recommendation treats watched titles across its selected participants.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchedPolicy {
    /// Exclude a title if any selected participant watched it.
    #[default]
    AnyParticipant,
    /// Exclude a title only if every selected participant watched it.
    AllParticipants,
    /// Do not exclude watched titles.
    IncludeAll,
}

