//! Meal planning domain and application foundation for the EME platform.
//!
//! The crate owns household identities, meal participation rules and persistence.
//! It is hosted inside the EME monorepo; HTTP handlers and agent tools remain thin
//! adapters, while authorization and request lifecycle belong to the EME runtime.

mod config;
mod domain;
mod error;
mod store;

pub use config::MealPlannerConfig;
pub use domain::{resolve_date, resolve_week, FamilyMember, MealArrangement, MealException, MealParticipation, MealType, MealWeekday, ParticipationResolver, ParticipationSource, ParticipationStatus, RecurringParticipationRule};
pub use error::MealPlannerError;
pub use store::MealPlannerStore;
