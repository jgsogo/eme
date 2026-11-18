//! Meal planning domain and application foundation for the EME platform.
//!
//! The crate owns household identities, meal participation rules, recipes and
//! persistence. HTTP handlers and agent tools remain thin adapters.
mod config;
mod domain;
mod error;
mod plans;
mod recipes;
mod school_menus;
mod shopping_lists;
mod store;

pub use config::MealPlannerConfig;
pub use domain::{
    FamilyMember, MealArrangement, MealException, MealParticipation, MealType, MealWeekday,
    ParticipationResolver, ParticipationSource, ParticipationStatus, RecurringParticipationRule,
    resolve_date, resolve_week,
};
pub use error::MealPlannerError;
pub use plans::{MealAssignment, PlanParticipant, WeeklyPlan, WeeklyPlanDraft, WeeklyPlanStatus};
pub use recipes::{Recipe, RecipeDraft, RecipeIngredient};
pub use school_menus::{
    SchoolMenuDraft, SchoolMenuEntry, SchoolMenuImportReport, SchoolMenuItem, SchoolMenuOrigin,
    SchoolMenuStatus,
};
pub use shopping_lists::{
    NewShoppingListItem, ShoppingList, ShoppingListItem, ShoppingListItemUpdate,
};
pub use store::MealPlannerStore;
