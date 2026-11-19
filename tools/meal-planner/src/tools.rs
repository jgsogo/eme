//! Explicit, independently registered agent tools for Meal Planner resources.
use crate::{
    MealArrangement, MealPlannerStore, MealType, MealWeekday, NewShoppingListItem, RecipeDraft,
    SchoolMenuDraft, ShoppingListItemUpdate, WeeklyPlanDraft,
};
use agent_runtime::{Tool, ToolContext, ToolError, ToolRegistry};
use async_trait::async_trait;
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::Value;
use std::{error::Error, path::PathBuf};

#[derive(Clone, Copy)]
enum Action {
    RecipeCreate,
    RecipeGet,
    RecipeList,
    RecipeUpdate,
    RecipeDelete,
    SchoolCreate,
    SchoolGet,
    SchoolList,
    SchoolUpdate,
    SchoolDelete,
    PlanContext,
    PlanSave,
    ParticipationRuleCreate,
    ParticipationExceptionSet,
    PlanCreate,
    PlanGet,
    PlanList,
    PlanUpdate,
    PlanDelete,
    ShoppingGet,
    ShoppingGenerate,
    ShoppingAdd,
    ShoppingUpdate,
    ShoppingRemove,
    ShoppingRestore,
}
struct MealTool {
    store: MealPlannerStore,
    action: Action,
}
fn scope(ctx: &ToolContext) -> Result<(String, String), ToolError> {
    let owner = ctx
        .user_id
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| {
            ToolError::Execution("Meal tools require an authenticated user context".into())
        })?
        .to_owned();
    let family = ctx
        .family
        .as_ref()
        .map(|v| v.family_id.trim())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            ToolError::Execution(
                "Meal tools require a selected family. Select a family for this conversation and retry.".into(),
            )
        })?
        .to_owned();
    Ok((owner, family))
}
fn sync_family_members(
    ctx: &ToolContext,
    store: &MealPlannerStore,
    family_id: &str,
) -> Result<(), ToolError> {
    let family = ctx.family.as_ref().ok_or_else(|| {
        failure("Meal tools require a selected family. Select a family for this conversation and retry.")
    })?;
    let members = family
        .members
        .iter()
        .map(|member| {
            let name = member
                .name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(&member.user_id);
            (member.user_id.clone(), name.to_owned())
        })
        .collect::<Vec<_>>();
    store
        .sync_family_members(family_id, &members)
        .map_err(failure)?;
    Ok(())
}
fn decode<T: for<'de> Deserialize<'de>>(v: Value) -> Result<T, ToolError> {
    serde_json::from_value(v)
        .map_err(|e| ToolError::Execution(format!("invalid tool arguments: {e}")))
}
fn encode(v: &impl serde::Serialize) -> Result<String, ToolError> {
    serde_json::to_string_pretty(v).map_err(|e| ToolError::Execution(e.to_string()))
}
fn failure(e: impl std::fmt::Display) -> ToolError {
    ToolError::Execution(e.to_string())
}
fn ensure_shopping_list_family(
    store: &MealPlannerStore,
    list_id: i64,
    family_id: &str,
) -> Result<(), ToolError> {
    if store
        .shopping_list_belongs_to_family(list_id, family_id)
        .map_err(failure)?
    {
        Ok(())
    } else {
        Err(failure(
            "shopping list does not belong to the selected family",
        ))
    }
}
#[derive(Deserialize)]
struct IdArgs {
    id: i64,
}
#[derive(Deserialize)]
struct DateArgs {
    date: NaiveDate,
}
#[derive(Deserialize)]
struct RangeArgs {
    start_date: NaiveDate,
    end_date: NaiveDate,
}
#[derive(Deserialize)]
struct WeekArgs {
    week_start: NaiveDate,
}
#[derive(Deserialize)]
struct PlanArgs {
    week_start: NaiveDate,
    assignments: Vec<crate::MealAssignment>,
}
#[derive(Deserialize)]
struct RecurringRuleArgs {
    member_id: i64,
    weekday: String,
    meal_type: MealType,
    arrangement: MealArrangement,
    effective_from: Option<NaiveDate>,
    effective_until: Option<NaiveDate>,
}
#[derive(Deserialize)]
struct ParticipationExceptionArgs {
    member_id: i64,
    date: NaiveDate,
    meal_type: MealType,
    arrangement: MealArrangement,
}
fn parse_weekday(value: &str) -> Result<MealWeekday, ToolError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "monday" | "mon" => Ok(MealWeekday::Monday),
        "tuesday" | "tue" => Ok(MealWeekday::Tuesday),
        "wednesday" | "wed" => Ok(MealWeekday::Wednesday),
        "thursday" | "thu" => Ok(MealWeekday::Thursday),
        "friday" | "fri" => Ok(MealWeekday::Friday),
        "saturday" | "sat" => Ok(MealWeekday::Saturday),
        "sunday" | "sun" => Ok(MealWeekday::Sunday),
        _ => Err(failure("weekday must be a weekday name, e.g. monday")),
    }
}
#[derive(Deserialize)]
struct SearchArgs {
    query: Option<String>,
}
#[derive(Deserialize)]
struct ItemArgs {
    list_id: i64,
    item_id: i64,
}
#[async_trait]
impl Tool for MealTool {
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<String, ToolError> {
        let (owner, family) = scope(ctx)?;
        sync_family_members(ctx, &self.store, &family)?;
        let families = vec![family.clone()];
        match self.action {
            Action::RecipeCreate => {
                let mut d: RecipeDraft = decode(args)?;
                if d.family_id.as_deref().is_some_and(|id| id != family) {
                    return Err(failure("recipe cannot be shared with a different family"));
                }
                d.family_id = Some(family.clone());
                encode(&self.store.create_recipe(&owner, d).map_err(failure)?)
            }
            Action::RecipeGet => {
                let a: IdArgs = decode(args)?;
                encode(
                    &self
                        .store
                        .get_recipe(a.id, &owner, &families)
                        .map_err(failure)?,
                )
            }
            Action::RecipeList => {
                let a: SearchArgs = decode(args)?;
                encode(
                    &self
                        .store
                        .list_recipes(&owner, &families, a.query.as_deref())
                        .map_err(failure)?,
                )
            }
            Action::RecipeUpdate => {
                #[derive(Deserialize)]
                struct A {
                    id: i64,
                    #[serde(flatten)]
                    draft: RecipeDraft,
                }
                let mut a: A = decode(args)?;
                if a.draft.family_id.as_deref().is_some_and(|id| id != family) {
                    return Err(failure("recipe cannot be moved to a different family"));
                }
                a.draft.family_id = Some(family.clone());
                encode(
                    &self
                        .store
                        .update_recipe_for_family(a.id, &owner, &family, a.draft)
                        .map_err(failure)?,
                )
            }
            Action::RecipeDelete => {
                let a: IdArgs = decode(args)?;
                self.store
                    .delete_recipe_for_family(a.id, &owner, &family)
                    .map_err(failure)?;
                Ok("{\"deleted\":true}".into())
            }
            Action::SchoolCreate => {
                let d: SchoolMenuDraft = decode(args)?;
                if self
                    .store
                    .get_school_menu_for_family(&family, d.date)
                    .map_err(failure)?
                    .is_some()
                {
                    return Err(failure(
                        "a school menu already exists for this date; use meals.school_menu.update",
                    ));
                }
                encode(
                    &self
                        .store
                        .save_school_menu_for_family(&family, d)
                        .map_err(failure)?,
                )
            }
            Action::SchoolGet => {
                let a: DateArgs = decode(args)?;
                encode(
                    &self
                        .store
                        .get_school_menu_for_family(&family, a.date)
                        .map_err(failure)?,
                )
            }
            Action::SchoolList => {
                let a: RangeArgs = decode(args)?;
                encode(
                    &self
                        .store
                        .list_school_menus_for_family(&family, a.start_date, a.end_date)
                        .map_err(failure)?,
                )
            }
            Action::SchoolUpdate => {
                let d: SchoolMenuDraft = decode(args)?;
                if self
                    .store
                    .get_school_menu_for_family(&family, d.date)
                    .map_err(failure)?
                    .is_none()
                {
                    return Err(failure(
                        "school menu does not exist; use meals.school_menu.create",
                    ));
                }
                encode(
                    &self
                        .store
                        .save_school_menu_for_family(&family, d)
                        .map_err(failure)?,
                )
            }
            Action::SchoolDelete => {
                let a: DateArgs = decode(args)?;
                self.store
                    .delete_school_menu_for_family(&family, a.date)
                    .map_err(failure)?;
                Ok("{\"deleted\":true}".into())
            }
            Action::PlanContext => {
                let a: WeekArgs = decode(args)?;
                let context = crate::MealPlannerApplication::new(self.store.clone())
                    .planning_context(&owner, &families, a.week_start)
                    .map_err(failure)?;
                encode(&context)
            }
            Action::ParticipationRuleCreate => {
                let a: RecurringRuleArgs = decode(args)?;
                encode(
                    &self
                        .store
                        .add_recurring_rule_for_family(
                            &family,
                            a.member_id,
                            parse_weekday(&a.weekday)?,
                            a.meal_type,
                            a.arrangement,
                            a.effective_from,
                            a.effective_until,
                        )
                        .map_err(failure)?,
                )
            }
            Action::ParticipationExceptionSet => {
                let a: ParticipationExceptionArgs = decode(args)?;
                encode(
                    &self
                        .store
                        .set_meal_exception_for_family(
                            &family,
                            a.member_id,
                            a.date,
                            a.meal_type,
                            a.arrangement,
                        )
                        .map_err(failure)?,
                )
            }
            Action::PlanSave => {
                let a: PlanArgs = decode(args)?;
                encode(
                    &self
                        .store
                        .save_weekly_plan_for_family(
                            &family,
                            a.week_start,
                            WeeklyPlanDraft {
                                assignments: a.assignments,
                            },
                        )
                        .map_err(failure)?,
                )
            }
            Action::PlanCreate => {
                let a: PlanArgs = decode(args)?;
                encode(
                    &self
                        .store
                        .create_weekly_plan_for_family(
                            &family,
                            a.week_start,
                            WeeklyPlanDraft {
                                assignments: a.assignments,
                            },
                        )
                        .map_err(failure)?,
                )
            }
            Action::PlanGet => {
                let a: WeekArgs = decode(args)?;
                encode(
                    &self
                        .store
                        .get_weekly_plan_for_family(&family, a.week_start)
                        .map_err(failure)?,
                )
            }
            Action::PlanList => {
                let a: RangeArgs = decode(args)?;
                encode(
                    &self
                        .store
                        .list_weekly_plans_for_family(&family, a.start_date, a.end_date)
                        .map_err(failure)?,
                )
            }
            Action::PlanUpdate => {
                let a: PlanArgs = decode(args)?;
                encode(
                    &self
                        .store
                        .update_weekly_plan_for_family(
                            &family,
                            a.week_start,
                            WeeklyPlanDraft {
                                assignments: a.assignments,
                            },
                        )
                        .map_err(failure)?,
                )
            }
            Action::PlanDelete => {
                let a: WeekArgs = decode(args)?;
                self.store
                    .delete_weekly_plan_for_family(&family, a.week_start)
                    .map_err(failure)?;
                Ok("{\"deleted\":true}".into())
            }
            Action::ShoppingGet => {
                let a: RangeArgs = decode(args)?;
                encode(
                    &self
                        .store
                        .get_shopping_list_for_range(&family, a.start_date, a.end_date)
                        .map_err(failure)?,
                )
            }
            Action::ShoppingGenerate => {
                let a: RangeArgs = decode(args)?;
                encode(
                    &self
                        .store
                        .generate_shopping_list_for_range(&family, a.start_date, a.end_date)
                        .map_err(failure)?,
                )
            }
            Action::ShoppingAdd => {
                #[derive(Deserialize)]
                struct B {
                    list_id: i64,
                    name: String,
                    quantity: Option<f64>,
                    unit: Option<String>,
                }
                let b: B = decode(args)?;
                ensure_shopping_list_family(&self.store, b.list_id, &family)?;
                encode(
                    &self
                        .store
                        .add_shopping_list_item(
                            b.list_id,
                            NewShoppingListItem {
                                name: b.name,
                                quantity: b.quantity,
                                unit: b.unit,
                            },
                        )
                        .map_err(failure)?,
                )
            }
            Action::ShoppingUpdate => {
                #[derive(Deserialize)]
                struct A {
                    list_id: i64,
                    item_id: i64,
                    name: String,
                    quantity: Option<f64>,
                    unit: Option<String>,
                    purchased: bool,
                }
                let a: A = decode(args)?;
                ensure_shopping_list_family(&self.store, a.list_id, &family)?;
                encode(
                    &self
                        .store
                        .update_shopping_list_item(
                            a.list_id,
                            a.item_id,
                            ShoppingListItemUpdate {
                                name: a.name,
                                quantity: a.quantity,
                                unit: a.unit,
                                purchased: a.purchased,
                            },
                        )
                        .map_err(failure)?,
                )
            }
            Action::ShoppingRemove => {
                let a: ItemArgs = decode(args)?;
                ensure_shopping_list_family(&self.store, a.list_id, &family)?;
                encode(
                    &self
                        .store
                        .remove_shopping_list_item(a.list_id, a.item_id)
                        .map_err(failure)?,
                )
            }
            Action::ShoppingRestore => {
                let a: ItemArgs = decode(args)?;
                ensure_shopping_list_family(&self.store, a.list_id, &family)?;
                encode(
                    &self
                        .store
                        .restore_shopping_list_item(a.list_id, a.item_id)
                        .map_err(failure)?,
                )
            }
        }
    }
}

pub fn register_tools(
    registry: &mut ToolRegistry,
    store: MealPlannerStore,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    let tools = [
        ("meal_recipe_create.toml", Action::RecipeCreate),
        ("meal_recipe_get.toml", Action::RecipeGet),
        ("meal_recipe_list.toml", Action::RecipeList),
        ("meal_recipe_update.toml", Action::RecipeUpdate),
        ("meal_recipe_delete.toml", Action::RecipeDelete),
        ("meal_school_menu_create.toml", Action::SchoolCreate),
        ("meal_school_menu_get.toml", Action::SchoolGet),
        ("meal_school_menu_list.toml", Action::SchoolList),
        ("meal_school_menu_update.toml", Action::SchoolUpdate),
        ("meal_school_menu_delete.toml", Action::SchoolDelete),
        ("meal_plan_context.toml", Action::PlanContext),
        ("meal_plan_save.toml", Action::PlanSave),
        (
            "meal_participation_rule_create.toml",
            Action::ParticipationRuleCreate,
        ),
        (
            "meal_participation_exception_set.toml",
            Action::ParticipationExceptionSet,
        ),
        ("meal_plan_create.toml", Action::PlanCreate),
        ("meal_plan_get.toml", Action::PlanGet),
        ("meal_plan_list.toml", Action::PlanList),
        ("meal_plan_update.toml", Action::PlanUpdate),
        ("meal_plan_delete.toml", Action::PlanDelete),
        ("meal_shopping_list_get.toml", Action::ShoppingGet),
        ("meal_shopping_list_generate.toml", Action::ShoppingGenerate),
        ("meal_shopping_list_add_item.toml", Action::ShoppingAdd),
        (
            "meal_shopping_list_update_item.toml",
            Action::ShoppingUpdate,
        ),
        (
            "meal_shopping_list_remove_item.toml",
            Action::ShoppingRemove,
        ),
        (
            "meal_shopping_list_restore_item.toml",
            Action::ShoppingRestore,
        ),
    ];
    for (file, action) in tools {
        registry.register_from_file(
            data.join(file),
            MealTool {
                store: store.clone(),
                action,
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn meal_tools_require_an_explicit_family_context() {
        let mut context = ToolContext::new("user-1");
        assert!(scope(&context).is_err());
        context.family = Some(agent_runtime::FamilyContext {
            family_id: "family-1".into(),
            family_name: "Home".into(),
            family_role: "owner".into(),
            relationship: "father".into(),
            members: vec![],
        });
        assert_eq!(
            scope(&context).unwrap(),
            ("user-1".into(), "family-1".into())
        );
        context
            .family
            .as_mut()
            .unwrap()
            .members
            .push(agent_runtime::FamilyMemberContext {
                user_id: "member-1".into(),
                name: Some("Alex".into()),
                birthday: None,
                telegram_id: None,
                role: "member".into(),
                relationship: "son".into(),
            });
        let store = MealPlannerStore::open_in_memory().unwrap();
        sync_family_members(&context, &store, "family-1").unwrap();
        let members = store.list_family_members_for_family("family-1").unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name, "Alex");
    }

    #[test]
    fn weekday_parser_accepts_full_and_short_names() {
        assert!(matches!(parse_weekday("Monday"), Ok(MealWeekday::Monday)));
        assert!(matches!(parse_weekday("thu"), Ok(MealWeekday::Thursday)));
        assert!(parse_weekday("someday").is_err());
    }

    #[test]
    fn shopping_list_mutations_are_scoped_to_the_selected_family() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let list = store
            .generate_shopping_list_for_range("family-a", date, date)
            .unwrap();

        assert!(ensure_shopping_list_family(&store, list.id, "family-a").is_ok());
        assert!(ensure_shopping_list_family(&store, list.id, "family-b").is_err());
        assert!(ensure_shopping_list_family(&store, i64::MAX, "family-a").is_err());
    }

    #[test]
    fn registers_explicit_resource_tools() {
        let mut r = ToolRegistry::new();
        register_tools(&mut r, MealPlannerStore::open_in_memory().unwrap()).unwrap();
        for name in [
            "meals.recipe.create",
            "meals.recipe.get",
            "meals.recipe.list",
            "meals.recipe.update",
            "meals.recipe.delete",
            "meals.school_menu.create",
            "meals.school_menu.get",
            "meals.school_menu.list",
            "meals.school_menu.update",
            "meals.school_menu.delete",
            "meals.plan.get_context",
            "meals.plan.save",
            "meals.participation.create_recurring_rule",
            "meals.participation.set_exception",
            "meals.plan.create",
            "meals.plan.get",
            "meals.plan.list",
            "meals.plan.update",
            "meals.plan.delete",
            "meals.shopping_list.get",
            "meals.shopping_list.generate",
            "meals.shopping_list.add_item",
            "meals.shopping_list.update_item",
            "meals.shopping_list.remove_item",
            "meals.shopping_list.restore_item",
        ] {
            assert!(r.contains(name), "missing tool {name}")
        }
        assert!(!r.contains("meals.plan.validate"));
        assert!(!r.contains("meals.plan.confirm"));
    }
}
