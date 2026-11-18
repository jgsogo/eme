//! SQLite persistence and user edits for generated shopping lists.
use crate::{
    MealPlannerError, MealPlannerStore,
    shopping_lists::{
        NewShoppingListItem, ShoppingList, ShoppingListItem, ShoppingListItemUpdate, generate_items,
    },
};
use rusqlite::{OptionalExtension, params};
use std::time::{SystemTime, UNIX_EPOCH};

impl MealPlannerStore {
    /// Generate or explicitly regenerate the list for one immutable plan revision.
    /// Existing user edits are reconciled rather than silently discarded.
    pub fn generate_shopping_list(&self, plan_id: i64) -> Result<ShoppingList, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let plan =
            super::plans::load_plan(&connection, plan_id)?.ok_or(MealPlannerError::NotFound)?;
        let mut recipes = std::collections::HashMap::new();
        for assignment in &plan.assignments {
            if assignment.arrangement == crate::MealArrangement::Home {
                if let Some(recipe_id) = assignment.recipe_id {
                    if !recipes.contains_key(&recipe_id) {
                        let recipe = super::recipes::load_recipe(&connection, recipe_id)?
                            .ok_or(MealPlannerError::NotFound)?;
                        recipes.insert(recipe_id, recipe);
                    }
                }
            }
        }
        let generated = generate_items(&plan, |id| recipes.get(&id).cloned())
            .map_err(MealPlannerError::InvalidInput)?;
        let current = load_shopping_list(&connection, plan_id)?;
        let same_plan = current.is_some();
        let existing = match current {
            Some(list) => Some(list),
            None => {
                load_latest_shopping_list_for_week(&connection, plan.week_start, plan.revision)?
            }
        };
        let now = timestamp();
        let mut list = ShoppingList::reconcile(existing, plan.id, plan.revision, generated, now);
        if !same_plan {
            // A new plan revision gets its own snapshot, seeded from the
            // previous revision's edits and purchase state.
            list.id = 0;
            list.created_at = now;
        }
        let payload = serde_json::to_string(&list.items)?;
        if list.id == 0 {
            connection.execute(
                "INSERT INTO shopping_lists(plan_id, week_start, plan_revision, items, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![plan.id, plan.week_start.to_string(), plan.revision, payload, now],
            )?;
        } else {
            connection.execute(
                "UPDATE shopping_lists SET plan_revision = ?1, items = ?2, updated_at = ?3 WHERE id = ?4",
                params![plan.revision, payload, now, list.id],
            )?;
        }
        load_shopping_list(&connection, plan_id)?.ok_or(MealPlannerError::NotFound)
    }

    pub fn get_shopping_list(
        &self,
        plan_id: i64,
    ) -> Result<Option<ShoppingList>, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        load_shopping_list(&connection, plan_id)
    }

    pub fn add_shopping_list_item(
        &self,
        plan_id: i64,
        item: NewShoppingListItem,
    ) -> Result<ShoppingList, MealPlannerError> {
        let item = item.validate().map_err(MealPlannerError::InvalidInput)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut list =
            load_shopping_list(&connection, plan_id)?.ok_or(MealPlannerError::NotFound)?;
        let id = list.items.iter().map(|item| item.id).max().unwrap_or(0) + 1;
        list.items.push(ShoppingListItem {
            id,
            name: item.name,
            quantity: item.quantity,
            unit: item.unit,
            purchased: false,
            source_assignment_ids: vec![],
            source_recipe_ids: vec![],
            manually_added: true,
            manually_removed: false,
            quantity_overridden: false,
            stale: false,
        });
        save_items(&connection, &mut list)?;
        Ok(list)
    }

    pub fn update_shopping_list_item(
        &self,
        plan_id: i64,
        item_id: i64,
        update: ShoppingListItemUpdate,
    ) -> Result<ShoppingList, MealPlannerError> {
        let update = update.validate().map_err(MealPlannerError::InvalidInput)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut list =
            load_shopping_list(&connection, plan_id)?.ok_or(MealPlannerError::NotFound)?;
        let item = list
            .items
            .iter_mut()
            .find(|item| item.id == item_id)
            .ok_or(MealPlannerError::NotFound)?;
        if item.name != update.name || item.quantity != update.quantity || item.unit != update.unit
        {
            item.quantity_overridden = true;
        }
        item.name = update.name;
        item.quantity = update.quantity;
        item.unit = update.unit;
        item.purchased = update.purchased;
        item.manually_removed = false;
        item.stale = false;
        save_items(&connection, &mut list)?;
        Ok(list)
    }

    pub fn remove_shopping_list_item(
        &self,
        plan_id: i64,
        item_id: i64,
    ) -> Result<ShoppingList, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut list =
            load_shopping_list(&connection, plan_id)?.ok_or(MealPlannerError::NotFound)?;
        let index = list
            .items
            .iter()
            .position(|item| item.id == item_id)
            .ok_or(MealPlannerError::NotFound)?;
        if list.items[index].manually_added {
            list.items.remove(index);
        } else {
            list.items[index].manually_removed = true;
            list.items[index].purchased = false;
        }
        save_items(&connection, &mut list)?;
        Ok(list)
    }
}

fn load_shopping_list(
    connection: &rusqlite::Connection,
    plan_id: i64,
) -> Result<Option<ShoppingList>, MealPlannerError> {
    let id = connection
        .query_row(
            "SELECT id FROM shopping_lists WHERE plan_id = ?1",
            [plan_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    match id {
        Some(id) => load_shopping_list_by_id(connection, id),
        None => Ok(None),
    }
}

fn load_latest_shopping_list_for_week(
    connection: &rusqlite::Connection,
    week_start: chrono::NaiveDate,
    before_revision: i64,
) -> Result<Option<ShoppingList>, MealPlannerError> {
    let id = connection.query_row(
        "SELECT id FROM shopping_lists WHERE week_start = ?1 AND plan_revision < ?2 ORDER BY plan_revision DESC, id DESC LIMIT 1",
        params![week_start.to_string(), before_revision],
        |row| row.get::<_, i64>(0),
    ).optional()?;
    match id {
        Some(id) => load_shopping_list_by_id(connection, id),
        None => Ok(None),
    }
}

fn load_shopping_list_by_id(
    connection: &rusqlite::Connection,
    id: i64,
) -> Result<Option<ShoppingList>, MealPlannerError> {
    Ok(connection.query_row(
        "SELECT id, plan_id, plan_revision, items, created_at, updated_at FROM shopping_lists WHERE id = ?1",
        [id],
        |row| {
            let payload: String = row.get(3)?;
            let items = serde_json::from_str(&payload).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(error))
            })?;
            Ok(ShoppingList {
                id: row.get(0)?,
                plan_id: row.get(1)?,
                plan_revision: row.get(2)?,
                items,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    ).optional()?)
}

fn save_items(
    connection: &rusqlite::Connection,
    list: &mut ShoppingList,
) -> Result<(), MealPlannerError> {
    list.updated_at = timestamp();
    let payload = serde_json::to_string(&list.items)?;
    connection.execute(
        "UPDATE shopping_lists SET items = ?1, updated_at = ?2 WHERE id = ?3",
        params![payload, list.updated_at, list.id],
    )?;
    Ok(())
}

fn timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MealArrangement, MealType, NewShoppingListItem, PlanParticipant, RecipeDraft,
        RecipeIngredient, ShoppingListItemUpdate, WeeklyPlanDraft,
    };
    use chrono::NaiveDate;

    #[test]
    fn generation_scales_and_user_edits_survive_explicit_regeneration() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let member_a = store.create_family_member("Alex").unwrap();
        let member_b = store.create_family_member("Sam").unwrap();
        let recipe = store
            .create_recipe("test-user", RecipeDraft {
                family_id: None,
                title: "Tomato pasta".into(),
                description: None,
                servings: 2,
                ingredients: vec![RecipeIngredient {
                    name: "Tomato".into(),
                    amount: Some(200.0),
                    unit: Some("g".into()),
                    preparation_note: None,
                }],
                steps: vec!["Cook".into()],
                categories: vec![],
                tags: vec![],
                source: None,
                notes: None,
                dietary_information: vec![],
                allergens: None,
            })
            .unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let plan = store
            .create_weekly_plan(
                date,
                WeeklyPlanDraft {
                    assignments: vec![crate::MealAssignment {
                        id: 1,
                        date,
                        meal_type: MealType::Lunch,
                        participants: vec![
                            PlanParticipant {
                                member_id: member_a.id,
                                name: member_a.name,
                            },
                            PlanParticipant {
                                member_id: member_b.id,
                                name: member_b.name,
                            },
                        ],
                        arrangement: MealArrangement::Home,
                        recipe_id: Some(recipe.id),
                        school_menu_date: None,
                        notes: None,
                    }],
                },
            )
            .unwrap();

        let generated = store.generate_shopping_list(plan.id).unwrap();
        assert_eq!(generated.items.len(), 1);
        assert_eq!(generated.items[0].quantity, Some(200.0));

        let manual = store
            .add_shopping_list_item(
                plan.id,
                NewShoppingListItem {
                    name: "Bread".into(),
                    quantity: None,
                    unit: None,
                },
            )
            .unwrap();
        let tomato_id = manual
            .items
            .iter()
            .find(|item| item.name == "Tomato")
            .unwrap()
            .id;
        let bread_id = manual
            .items
            .iter()
            .find(|item| item.name == "Bread")
            .unwrap()
            .id;
        store
            .update_shopping_list_item(
                plan.id,
                tomato_id,
                ShoppingListItemUpdate {
                    name: "Tomato".into(),
                    quantity: Some(3.0),
                    unit: Some("kg".into()),
                    purchased: true,
                },
            )
            .unwrap();
        store.remove_shopping_list_item(plan.id, tomato_id).unwrap();

        let regenerated = store.generate_shopping_list(plan.id).unwrap();
        let tomato = regenerated
            .items
            .iter()
            .find(|item| item.id == tomato_id)
            .unwrap();
        assert!(tomato.manually_removed);
        assert!(tomato.quantity_overridden);
        assert_eq!(tomato.quantity, Some(3.0));
        assert!(!tomato.purchased);
        assert!(
            regenerated
                .items
                .iter()
                .any(|item| item.id == bread_id && item.manually_added)
        );
        assert_eq!(regenerated.plan_revision, plan.revision);

        // A new immutable plan revision receives a separate list snapshot,
        // carrying manual additions, removals and quantity overrides forward.
        store.confirm_weekly_plan(date).unwrap();
        let revision = store.revise_weekly_plan(date).unwrap();
        let mut revised_assignment = revision.assignments[0].clone();
        revised_assignment.id = 2;
        revised_assignment.meal_type = MealType::Dinner;
        revised_assignment.participants.truncate(1);
        store
            .update_weekly_plan(
                date,
                WeeklyPlanDraft {
                    assignments: vec![revision.assignments[0].clone(), revised_assignment],
                },
            )
            .unwrap();
        let next_list = store.generate_shopping_list(revision.id).unwrap();
        assert_ne!(next_list.id, regenerated.id);
        assert_eq!(next_list.plan_id, revision.id);
        let next_tomato = next_list
            .items
            .iter()
            .find(|item| item.name == "Tomato")
            .unwrap();
        assert!(next_tomato.manually_removed);
        assert!(next_tomato.quantity_overridden);
        assert_eq!(next_tomato.quantity, Some(3.0));
        assert!(
            next_list
                .items
                .iter()
                .any(|item| item.id == bread_id && item.manually_added)
        );
        assert_eq!(
            store.get_shopping_list(plan.id).unwrap().unwrap().id,
            regenerated.id
        );
    }

    #[test]
    fn lists_are_separate_from_plans_and_unknown_quantities_are_retained() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let member = store.create_family_member("Alex").unwrap();
        let recipe = store
            .create_recipe("test-user", RecipeDraft {
                family_id: None,
                title: "Soup".into(),
                description: None,
                servings: 1,
                ingredients: vec![RecipeIngredient {
                    name: "Salt".into(),
                    amount: None,
                    unit: None,
                    preparation_note: None,
                }],
                steps: vec!["Cook".into()],
                categories: vec![],
                tags: vec![],
                source: None,
                notes: None,
                dietary_information: vec![],
                allergens: None,
            })
            .unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let plan = store
            .create_weekly_plan(
                date,
                WeeklyPlanDraft {
                    assignments: vec![crate::MealAssignment {
                        id: 1,
                        date,
                        meal_type: MealType::Lunch,
                        participants: vec![PlanParticipant {
                            member_id: member.id,
                            name: member.name,
                        }],
                        arrangement: MealArrangement::Home,
                        recipe_id: Some(recipe.id),
                        school_menu_date: None,
                        notes: None,
                    }],
                },
            )
            .unwrap();
        let list = store.generate_shopping_list(plan.id).unwrap();
        assert_eq!(list.items[0].quantity, None);
        assert_eq!(store.get_weekly_plan(date).unwrap().unwrap().id, plan.id);
        assert_eq!(
            store.get_shopping_list(plan.id).unwrap().unwrap().id,
            list.id
        );
    }
}
