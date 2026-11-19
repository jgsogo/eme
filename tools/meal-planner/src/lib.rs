//! Meal planning domain and persistence foundation for EME.
mod application;
mod config;
mod domain;
mod error;
mod plans;
mod recipes;
mod school_menus;
mod shopping_lists;
mod store;
mod tools;

pub use application::{MealPlannerApplication, PlanningContext, PlanningParticipation};
pub use config::MealPlannerConfig;
pub use domain::{
    FamilyMember, MealArrangement, MealException, MealParticipation, MealType, MealWeekday,
    ParticipationResolver, ParticipationSource, ParticipationStatus, RecurringParticipationRule,
    resolve_date, resolve_week,
};
pub use error::MealPlannerError;
pub use plans::{MealAssignment, PlanParticipant, WeeklyPlan, WeeklyPlanDraft};
pub use recipes::{Recipe, RecipeDraft, RecipeIngredient};
pub use school_menus::{SchoolMenuDraft, SchoolMenuEntry, SchoolMenuItem, SchoolMenuStatus};
pub use shopping_lists::{
    NewShoppingListItem, ShoppingList, ShoppingListItem, ShoppingListItemUpdate,
};
pub use store::MealPlannerStore;
pub use tools::register_tools;
