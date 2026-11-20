//! Commercial type of a streaming offer.
use serde::{Deserialize, Serialize};

/// How a title can be accessed from a streaming provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferType {
    Subscription,
    Rent,
    Buy,
    Free,
}
