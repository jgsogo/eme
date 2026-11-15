use thiserror::Error;
use super::RunStatus;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid run state transition: {from:?} -> {to:?}")]
pub struct InvalidRunTransition {
    pub from: RunStatus,
    pub to: RunStatus,
}
