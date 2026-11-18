//! Meal planning domain and application foundation for the EME platform.
//!
//! This crate owns the Meal Planner's domain rules, application use cases and
//! persistence as those capabilities are introduced. It is hosted in the EME
//! monorepo and is designed as a modular application module, not a separately
//! deployed service.
//!
//! The initial public surface provides configuration, a SQLite-backed store
//! and stable application errors. Domain features and their use cases are
//! added in focused modules; agent tools and HTTP handlers remain adapters,
//! while authorization and request lifecycle stay with the EME runtime/server.

mod config;
mod error;
mod store;

pub use config::MealPlannerConfig;
pub use error::MealPlannerError;
pub use store::MealPlannerStore;
