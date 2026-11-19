//! SQLite persistence for date-range shopping lists.
use super::plans::load_plan;
use super::recipes::load_recipe;
use crate::shopping_lists::generate_items;
use crate::{
    MealPlannerError, MealPlannerStore, NewShoppingListItem, ShoppingList, ShoppingListItem,
    ShoppingListItemUpdate,
};
use chrono::{Datelike, NaiveDate, Weekday};
use rusqlite::{OptionalExtension, params};
use std::time::{SystemTime, UNIX_EPOCH};

impl MealPlannerStore {
    pub fn generate_shopping_list_for_range(
        &self,
        family_id: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<ShoppingList, MealPlannerError> {
        validate_range(start, end)?;
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let first = monday(start);
        let last = monday(end);
        let mut st=c.prepare("SELECT id FROM weekly_plans WHERE family_id=?1 AND week_start>=?2 AND week_start<=?3 ORDER BY week_start")?;
        let ids = st
            .query_map(
                params![family_id, first.to_string(), last.to_string()],
                |r| r.get::<_, i64>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let mut assignments = Vec::new();
        let mut recipes = std::collections::HashMap::new();
        for id in ids {
            let plan = load_plan(&c, id)?.ok_or(MealPlannerError::NotFound)?;
            for a in plan
                .assignments
                .into_iter()
                .filter(|a| a.date >= start && a.date <= end)
            {
                for recipe_id in &a.recipe_ids {
                    if !recipes.contains_key(recipe_id) {
                        recipes.insert(
                            *recipe_id,
                            load_recipe(&c, *recipe_id)?.ok_or(MealPlannerError::NotFound)?,
                        );
                    }
                }
                assignments.push(a)
            }
        }
        let generated = generate_items(&assignments, |id| recipes.get(&id).cloned())
            .map_err(MealPlannerError::InvalidInput)?;
        let existing = load_range(&c, family_id, start, end)?;
        let now = timestamp();
        let list =
            ShoppingList::reconcile(existing, family_id.to_owned(), start, end, generated, now);
        let payload = serde_json::to_string(&list.items)?;
        if list.id == 0 {
            c.execute("INSERT INTO shopping_lists(family_id,start_date,end_date,items,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5)",params![family_id,start.to_string(),end.to_string(),payload,now])?;
        } else {
            c.execute(
                "UPDATE shopping_lists SET items=?1,updated_at=?2 WHERE id=?3",
                params![payload, now, list.id],
            )?;
        }
        load_by_id(
            &c,
            if list.id == 0 {
                c.last_insert_rowid()
            } else {
                list.id
            },
        )?
        .ok_or(MealPlannerError::NotFound)
    }
    pub fn get_shopping_list_for_range(
        &self,
        family_id: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Option<ShoppingList>, MealPlannerError> {
        validate_range(start, end)?;
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        load_range(&c, family_id, start, end)
    }
    pub fn shopping_list_belongs_to_family(
        &self,
        id: i64,
        family_id: &str,
    ) -> Result<bool, MealPlannerError> {
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        Ok(c.query_row(
            "SELECT EXISTS(SELECT 1 FROM shopping_lists WHERE id=?1 AND family_id=?2)",
            params![id, family_id],
            |r| r.get(0),
        )?)
    }
    pub fn add_shopping_list_item(
        &self,
        list_id: i64,
        item: NewShoppingListItem,
    ) -> Result<ShoppingList, MealPlannerError> {
        let item = item.validate().map_err(MealPlannerError::InvalidInput)?;
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut list = load_by_id(&c, list_id)?.ok_or(MealPlannerError::NotFound)?;
        let id = list.items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
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
        save(&c, &mut list)?;
        Ok(list)
    }
    pub fn update_shopping_list_item(
        &self,
        list_id: i64,
        item_id: i64,
        update: ShoppingListItemUpdate,
    ) -> Result<ShoppingList, MealPlannerError> {
        let update = update.validate().map_err(MealPlannerError::InvalidInput)?;
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut list = load_by_id(&c, list_id)?.ok_or(MealPlannerError::NotFound)?;
        let item = list
            .items
            .iter_mut()
            .find(|i| i.id == item_id)
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
        save(&c, &mut list)?;
        Ok(list)
    }
    pub fn remove_shopping_list_item(
        &self,
        list_id: i64,
        item_id: i64,
    ) -> Result<ShoppingList, MealPlannerError> {
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut list = load_by_id(&c, list_id)?.ok_or(MealPlannerError::NotFound)?;
        let ix = list
            .items
            .iter()
            .position(|i| i.id == item_id)
            .ok_or(MealPlannerError::NotFound)?;
        if list.items[ix].manually_added {
            list.items.remove(ix);
        } else {
            list.items[ix].manually_removed = true;
            list.items[ix].purchased = false;
        }
        save(&c, &mut list)?;
        Ok(list)
    }
    pub fn restore_shopping_list_item(
        &self,
        list_id: i64,
        item_id: i64,
    ) -> Result<ShoppingList, MealPlannerError> {
        let c = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut list = load_by_id(&c, list_id)?.ok_or(MealPlannerError::NotFound)?;
        let first = monday(list.start_date);
        let last = monday(list.end_date);
        let mut st = c.prepare(
            "SELECT id FROM weekly_plans WHERE family_id=?1 AND week_start>=?2 AND week_start<=?3",
        )?;
        let ids = st
            .query_map(
                params![list.family_id, first.to_string(), last.to_string()],
                |r| r.get::<_, i64>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let mut assignments = Vec::new();
        let mut recipes = std::collections::HashMap::new();
        for id in ids {
            let p = load_plan(&c, id)?.ok_or(MealPlannerError::NotFound)?;
            for a in p
                .assignments
                .into_iter()
                .filter(|a| a.date >= list.start_date && a.date <= list.end_date)
            {
                for rid in &a.recipe_ids {
                    if !recipes.contains_key(rid) {
                        recipes.insert(
                            *rid,
                            load_recipe(&c, *rid)?.ok_or(MealPlannerError::NotFound)?,
                        );
                    }
                }
                assignments.push(a)
            }
        }
        let generated = generate_items(&assignments, |id| recipes.get(&id).cloned())
            .map_err(MealPlannerError::InvalidInput)?;
        let item = list
            .items
            .iter_mut()
            .find(|i| i.id == item_id)
            .ok_or(MealPlannerError::NotFound)?;
        if item.manually_added {
            return Err(MealPlannerError::InvalidInput(
                "manually added items have no recipe quantity to restore".into(),
            ));
        }
        let original = generated
            .into_iter()
            .find(|g| {
                g.name.eq_ignore_ascii_case(&item.name)
                    && g.source_recipe_ids == item.source_recipe_ids
                    && g.source_assignment_ids == item.source_assignment_ids
            })
            .ok_or(MealPlannerError::NotFound)?;
        item.quantity = original.quantity;
        item.unit = original.unit;
        item.quantity_overridden = false;
        item.manually_removed = false;
        item.stale = false;
        save(&c, &mut list)?;
        Ok(list)
    }
}
fn validate_range(start: NaiveDate, end: NaiveDate) -> Result<(), MealPlannerError> {
    if end < start {
        Err(MealPlannerError::InvalidInput(
            "shopping-list range ends before it starts".into(),
        ))
    } else {
        Ok(())
    }
}
fn monday(d: NaiveDate) -> NaiveDate {
    d - chrono::Duration::days(d.weekday().num_days_from_monday() as i64)
}
fn load_range(
    c: &rusqlite::Connection,
    family: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Option<ShoppingList>, MealPlannerError> {
    let id = c
        .query_row(
            "SELECT id FROM shopping_lists WHERE family_id=?1 AND start_date=?2 AND end_date=?3",
            params![family, start.to_string(), end.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    id.map(|id| load_by_id(c, id)?.ok_or(MealPlannerError::NotFound))
        .transpose()
}
fn load_by_id(c: &rusqlite::Connection, id: i64) -> Result<Option<ShoppingList>, MealPlannerError> {
    Ok(c.query_row("SELECT id,family_id,start_date,end_date,items,created_at,updated_at FROM shopping_lists WHERE id=?1",[id],|r|{let parse=|i|->rusqlite::Result<NaiveDate>{NaiveDate::parse_from_str(&r.get::<_,String>(i)?,"%Y-%m-%d").map_err(|e|rusqlite::Error::FromSqlConversionFailure(i,rusqlite::types::Type::Text,Box::new(e)))};let raw:String=r.get(4)?;let items=serde_json::from_str(&raw).map_err(|e|rusqlite::Error::FromSqlConversionFailure(4,rusqlite::types::Type::Text,Box::new(e)))?;Ok(ShoppingList{id:r.get(0)?,family_id:r.get(1)?,start_date:parse(2)?,end_date:parse(3)?,items,created_at:r.get(5)?,updated_at:r.get(6)?})}).optional()?)
}
fn save(c: &rusqlite::Connection, list: &mut ShoppingList) -> Result<(), MealPlannerError> {
    list.updated_at = timestamp();
    let payload = serde_json::to_string(&list.items)?;
    c.execute(
        "UPDATE shopping_lists SET items=?1,updated_at=?2 WHERE id=?3",
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
        MealPlannerStore, MealType, PlanParticipant, RecipeDraft, RecipeIngredient, WeeklyPlanDraft,
    };
    use chrono::NaiveDate;
    #[test]
    fn range_generation_aggregates_plan_ingredients_and_preserves_disabled_items() {
        let store = MealPlannerStore::open_in_memory().unwrap();
        let member = store.create_family_member("Alex").unwrap();
        let recipe = store
            .create_recipe(
                "owner",
                RecipeDraft {
                    family_id: Some("legacy".into()),
                    title: "Soup".into(),
                    description: None,
                    servings: 1,
                    ingredients: vec![
                        RecipeIngredient {
                            name: "Carrot".into(),
                            amount: Some(2.0),
                            unit: Some("pcs".into()),
                            preparation_note: None,
                        },
                        RecipeIngredient {
                            name: "Eggs".into(),
                            amount: Some(2.0),
                            unit: None,
                            preparation_note: None,
                        },
                        RecipeIngredient {
                            name: " eggs ".into(),
                            amount: Some(3.0),
                            unit: None,
                            preparation_note: None,
                        },
                    ],
                    steps: vec!["Cook".into()],
                    categories: vec![],
                    tags: vec![],
                    source: None,
                    notes: None,
                    dietary_information: vec![],
                    allergens: None,
                },
            )
            .unwrap();
        let monday = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        store
            .create_weekly_plan(
                monday,
                WeeklyPlanDraft {
                    assignments: vec![crate::MealAssignment {
                        id: 1,
                        date: monday,
                        meal_type: MealType::Lunch,
                        participants: vec![PlanParticipant {
                            member_id: member.id,
                            name: member.name,
                        }],
                        recipe_ids: vec![recipe.id],
                    }],
                },
            )
            .unwrap();
        let mut list = store
            .generate_shopping_list_for_range("legacy", monday, monday.succ_opt().unwrap())
            .unwrap();
        assert_eq!(list.items.len(), 2);
        let carrot = list.items.iter().find(|item| item.name == "Carrot").unwrap();
        assert_eq!(carrot.quantity, Some(2.0));
        let eggs = list.items.iter().find(|item| item.name == "Eggs").unwrap();
        assert_eq!(eggs.quantity, Some(5.0));
        assert_eq!(eggs.unit, None);
        let id = carrot.id;
        list = store.remove_shopping_list_item(list.id, id).unwrap();
        assert!(list.items.iter().find(|item| item.id == id).unwrap().manually_removed);
        list = store
            .generate_shopping_list_for_range("legacy", monday, monday.succ_opt().unwrap())
            .unwrap();
        assert!(list.items[0].manually_removed);
    }
}
