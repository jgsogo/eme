//! SQLite persistence and user edits for generated shopping lists.
use crate::{
    MealPlannerError, MealPlannerStore,
    shopping_lists::{NewShoppingListItem, ShoppingList, ShoppingListItem, ShoppingListItemUpdate, generate_items},
};
use rusqlite::{OptionalExtension, params};
use std::time::{SystemTime, UNIX_EPOCH};

impl MealPlannerStore {
    /// Generate or explicitly regenerate the list for one immutable plan revision.
    /// Existing user edits are reconciled rather than silently discarded.
    pub fn generate_shopping_list(&self, plan_id: i64) -> Result<ShoppingList, MealPlannerError> {
        let connection = self.connection.lock().map_err(|_| MealPlannerError::LockPoisoned)?;
        let plan = super::plans::load_plan(&connection, plan_id)?.ok_or(MealPlannerError::NotFound)?;
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
        let existing = load_shopping_list(&connection, plan_id)?;
        let now = timestamp();
        let list = ShoppingList::reconcile(existing, plan.id, plan.revision, generated, now);
        let payload = serde_json::to_string(&list.items)?;
        if list.id == 0 {
            connection.execute(
                "INSERT INTO shopping_lists(plan_id, plan_revision, items, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                params![plan.id, plan.revision, payload, now],
            )?;
        } else {
            connection.execute(
                "UPDATE shopping_lists SET plan_revision = ?1, items = ?2, updated_at = ?3 WHERE id = ?4",
                params![plan.revision, payload, now, list.id],
            )?;
        }
        load_shopping_list(&connection, plan_id)?.ok_or(MealPlannerError::NotFound)
    }

    pub fn get_shopping_list(&self, plan_id: i64) -> Result<Option<ShoppingList>, MealPlannerError> {
        let connection = self.connection.lock().map_err(|_| MealPlannerError::LockPoisoned)?;
        load_shopping_list(&connection, plan_id)
    }

    pub fn add_shopping_list_item(
        &self,
        plan_id: i64,
        item: NewShoppingListItem,
    ) -> Result<ShoppingList, MealPlannerError> {
        let item = item.validate().map_err(MealPlannerError::InvalidInput)?;
        let connection = self.connection.lock().map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut list = load_shopping_list(&connection, plan_id)?.ok_or(MealPlannerError::NotFound)?;
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
        let connection = self.connection.lock().map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut list = load_shopping_list(&connection, plan_id)?.ok_or(MealPlannerError::NotFound)?;
        let item = list.items.iter_mut().find(|item| item.id == item_id).ok_or(MealPlannerError::NotFound)?;
        if item.name != update.name || item.quantity != update.quantity || item.unit != update.unit {
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
        let connection = self.connection.lock().map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut list = load_shopping_list(&connection, plan_id)?.ok_or(MealPlannerError::NotFound)?;
        let index = list.items.iter().position(|item| item.id == item_id).ok_or(MealPlannerError::NotFound)?;
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
    Ok(connection.query_row(
        "SELECT id, plan_revision, items, created_at, updated_at FROM shopping_lists WHERE plan_id = ?1",
        [plan_id],
        |row| {
            let payload: String = row.get(2)?;
            let items = serde_json::from_str(&payload).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(error))
            })?;
            Ok(ShoppingList {
                id: row.get(0)?,
                plan_id,
                plan_revision: row.get(1)?,
                items,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
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
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}
