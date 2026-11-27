//! Classification of the access terms associated with a model.

/// Commercial access classification for a model.
///
/// This is descriptive catalogue metadata, not a billing calculation. Token
/// prices and cost estimation remain the responsibility of `agent-metering`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessTier {
    /// The provider documents this model as available without payment.
    Free,
    /// The provider offers a limited free allocation or free quota.
    FreeTier,
    /// The provider offers temporary or evaluation access.
    Trial,
    /// Use of the model requires paid access.
    Paid,
    /// The access terms have not yet been classified.
    #[default]
    Unknown,
}
