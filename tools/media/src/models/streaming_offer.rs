//! Normalized streaming availability offer.
use serde::{Deserialize, Serialize};

use super::{MediaItemId, OfferType};

/// Availability offer returned by a provider adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamingOffer {
    pub media_item_id: MediaItemId,
    /// Provider/source identifier resolved from provider metadata, not a hardcoded enum.
    pub service_id: String,
    pub service_name: String,
    pub offer_type: OfferType,
    pub region: String,
    pub url: Option<String>,
}
