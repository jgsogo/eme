use crate::ErrorCategory;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStatus {
    Completed,
    MaxIterationsReached,
    Cancelled,
    TimedOut,
}

impl RunStatus {
    pub fn error_category(&self) -> Option<ErrorCategory> {
        match self {
            Self::Cancelled => Some(ErrorCategory::Cancellation),
            Self::TimedOut => Some(ErrorCategory::Timeout),
            Self::Completed | Self::MaxIterationsReached => None,
        }
    }
}

