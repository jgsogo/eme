//! Errors returned by provider clients and model resolution.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelFailureCategory {
    RateLimited,
    QuotaExceeded,
    Transient,
    CandidateUnavailable,
    Permanent,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("Model name is invalid: {0}")]
    InvalidModelName(String),

    #[error("Missing API key: {0}")]
    MissingApiKey(String),

    #[error("Provider '{0}' is not supported")]
    UnsupportedProvider(String),

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Provider returned an error: {0}")]
    Provider(String),

    #[error("Provider returned HTTP {status}: {body}")]
    ProviderResponse { status: u16, body: String },

    #[error("Invalid provider response: {0}")]
    InvalidResponse(String),

    #[error("Unsupported model '{0}'")]
    UnsupportedModel(String),

    #[error("Model routing was cancelled")]
    Cancelled,

    #[error("All compatible model candidates failed: {0}")]
    AllCandidatesFailed(String),
}

impl ModelError {
    /// Classifies whether another compatible model may succeed for this request.
    ///
    /// Unknown provider errors are deliberately permanent: fallback is only
    /// attempted when the provider response or transport error gives a safe
    /// reason to believe another candidate may work.
    pub fn failure_category(&self) -> ModelFailureCategory {
        match self {
            Self::MissingApiKey(_) | Self::UnsupportedProvider(_) | Self::UnsupportedModel(_) => {
                ModelFailureCategory::CandidateUnavailable
            }
            Self::Http(error) if error.is_timeout() || error.is_connect() => {
                ModelFailureCategory::Transient
            }
            Self::ProviderResponse { status, body } => match *status {
                408 | 425 | 500..=599 => ModelFailureCategory::Transient,
                429 if body.to_ascii_lowercase().contains("quota") => {
                    ModelFailureCategory::QuotaExceeded
                }
                429 => ModelFailureCategory::RateLimited,
                401 | 403 | 404 => ModelFailureCategory::CandidateUnavailable,
                _ => ModelFailureCategory::Permanent,
            },
            Self::Http(_)
            | Self::InvalidModelName(_)
            | Self::Provider(_)
            | Self::InvalidResponse(_)
            | Self::Cancelled
            | Self::AllCandidatesFailed(_) => ModelFailureCategory::Permanent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ModelError, ModelFailureCategory};

    #[test]
    fn classifies_provider_failures_without_retrying_permanent_request_errors() {
        assert_eq!(
            ModelError::ProviderResponse {
                status: 429,
                body: r#"{"error":{"type":"rate_limit_error"}}"#.into(),
            }
            .failure_category(),
            ModelFailureCategory::RateLimited
        );
        assert_eq!(
            ModelError::ProviderResponse {
                status: 429,
                body: r#"{"error":{"code":"insufficient_quota"}}"#.into(),
            }
            .failure_category(),
            ModelFailureCategory::QuotaExceeded
        );
        assert_eq!(
            ModelError::ProviderResponse {
                status: 503,
                body: "service unavailable".into(),
            }
            .failure_category(),
            ModelFailureCategory::Transient
        );
        assert_eq!(
            ModelError::ProviderResponse {
                status: 401,
                body: "invalid credentials".into(),
            }
            .failure_category(),
            ModelFailureCategory::CandidateUnavailable
        );
        assert_eq!(
            ModelError::ProviderResponse {
                status: 400,
                body: "invalid request".into(),
            }
            .failure_category(),
            ModelFailureCategory::Permanent
        );
        assert_eq!(
            ModelError::InvalidResponse("malformed response".into()).failure_category(),
            ModelFailureCategory::Permanent
        );
        assert_eq!(
            ModelError::MissingApiKey("PROVIDER_KEY".into()).failure_category(),
            ModelFailureCategory::CandidateUnavailable
        );
    }
}
