//! Family membership workflows and authorization policy.
//!
//! This module exposes the family service as the application-facing boundary
//! for family operations. It keeps authorization decisions out of transports
//! and delegates durable state changes to the storage interfaces.

mod family_service;
mod family_service_error;
#[cfg(test)]
mod tests;

pub use family_service::FamilyService;
pub use family_service_error::FamilyServiceError;
