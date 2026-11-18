//! Meal planning domain and application foundation for the EME platform.
//!
//! The crate owns household identities, meal participation rules, recipes and
//! persistence. HTTP handlers and agent tools remain thin adapters.
mod config;
mod domain;
mod error;
mod recipes;
mod store;

pub use config::MealPlannerConfig;
pub use domain::{
    FamilyMember, MealArrangement, MealException, MealParticipation, MealType, MealWeekday,
    ParticipationResolver, ParticipationSource, ParticipationStatus, RecurringParticipationRule,
    resolve_date, resolve_week,
};
pub use error::MealPlannerError;
pub use recipes::{Recipe, RecipeDraft, RecipeIngredient};
pub use store::MealPlannerStore;
