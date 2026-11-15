/// Stable error classes for runtime decisions and observability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Configuration,
    Authorization,
    Model,
    Tool,
    Timeout,
    Cancellation,
}
