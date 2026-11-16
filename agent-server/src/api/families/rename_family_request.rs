//! Payload for changing a family's display name.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RenameFamilyRequest {
    pub name: String,
}
