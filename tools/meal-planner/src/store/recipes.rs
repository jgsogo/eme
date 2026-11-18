//! SQLite persistence operations for the recipe catalogue.
use crate::{MealPlannerError, MealPlannerStore, Recipe, RecipeDraft};
use rusqlite::{OptionalExtension, params};
use std::time::{SystemTime, UNIX_EPOCH};

impl MealPlannerStore {
    /// Create a validated recipe and return its stable database identity.
    pub fn create_recipe(&self, draft: RecipeDraft) -> Result<Recipe, MealPlannerError> {
        let draft = draft.validate().map_err(MealPlannerError::InvalidInput)?;
        let payload = serde_json::to_string(&draft)?;
        let now = timestamp();
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO recipes(title, payload, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
            params![draft.title, payload, now],
        )?;
        let id = transaction.last_insert_rowid();
        transaction.commit()?;
        Ok(Recipe {
            id,
            draft,
            created_at: now,
            updated_at: now,
        })
    }

    /// Replace editable content while retaining the original creation timestamp.
    pub fn update_recipe(&self, id: i64, draft: RecipeDraft) -> Result<Recipe, MealPlannerError> {
        let draft = draft.validate().map_err(MealPlannerError::InvalidInput)?;
        let payload = serde_json::to_string(&draft)?;
        let now = timestamp();
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let changed = connection.execute(
            "UPDATE recipes SET title = ?1, payload = ?2, updated_at = ?3 WHERE id = ?4",
            params![draft.title, payload, now, id],
        )?;
        if changed == 0 {
            return Err(MealPlannerError::NotFound);
        }
        load_recipe(&connection, id)?.ok_or(MealPlannerError::NotFound)
    }

    pub fn get_recipe(&self, id: i64) -> Result<Recipe, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        load_recipe(&connection, id)?.ok_or(MealPlannerError::NotFound)
    }

    /// List recipes, optionally filtering by title, description, ingredient, category or tag.
    pub fn list_recipes(&self, query: Option<&str>) -> Result<Vec<Recipe>, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT id, payload, created_at, updated_at FROM recipes ORDER BY lower(title), id",
        )?;
        let recipes = statement
            .query_map([], |row| {
                let payload: String = row.get(1)?;
                let draft = serde_json::from_str(&payload).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                Ok(Recipe {
                    id: row.get(0)?,
                    draft,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
            return Ok(recipes);
        };
        let needle = query.to_lowercase();
        Ok(recipes
            .into_iter()
            .filter(|recipe| {
                let draft = &recipe.draft;
                draft.title.to_lowercase().contains(&needle)
                    || draft
                        .description
                        .as_deref()
                        .is_some_and(|value| value.to_lowercase().contains(&needle))
                    || draft
                        .ingredients
                        .iter()
                        .any(|item| item.name.to_lowercase().contains(&needle))
                    || draft
                        .tags
                        .iter()
                        .chain(&draft.categories)
                        .any(|value| value.to_lowercase().contains(&needle))
            })
            .collect())
    }

    pub fn delete_recipe(&self, id: i64) -> Result<(), MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        if connection.execute("DELETE FROM recipes WHERE id = ?1", [id])? == 0 {
            return Err(MealPlannerError::NotFound);
        }
        Ok(())
    }
}

pub(super) fn load_recipe(
    connection: &rusqlite::Connection,
    id: i64,
) -> Result<Option<Recipe>, MealPlannerError> {
    Ok(connection
        .query_row(
            "SELECT id, payload, created_at, updated_at FROM recipes WHERE id = ?1",
            [id],
            |row| {
                let payload: String = row.get(1)?;
                let draft = serde_json::from_str(&payload).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                Ok(Recipe {
                    id: row.get(0)?,
                    draft,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            },
        )
        .optional()?)
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
    use crate::RecipeIngredient;

    fn store() -> MealPlannerStore {
        MealPlannerStore::open_in_memory().unwrap()
    }

    fn draft(title: &str, ingredient: &str) -> RecipeDraft {
        RecipeDraft {
            title: title.into(),
            description: Some("A simple meal".into()),
            servings: 2,
            ingredients: vec![RecipeIngredient {
                name: ingredient.into(),
                amount: Some(200.0),
                unit: Some("g".into()),
                preparation_note: None,
            }],
            steps: vec!["Prepare ingredients".into(), "Cook until ready".into()],
            categories: vec!["Dinner".into()],
            tags: vec!["quick".into()],
            source: None,
            notes: None,
            dietary_information: vec![],
            allergens: None,
        }
    }

    #[test]
    fn recipes_can_be_searched_updated_and_deleted() {
        let store = store();
        let recipe = store
            .create_recipe(draft("Tomato pasta", "tomato"))
            .unwrap();
        assert_eq!(store.get_recipe(recipe.id).unwrap(), recipe);
        assert_eq!(store.list_recipes(Some("TOMATO")).unwrap().len(), 1);
        let updated = store
            .update_recipe(recipe.id, draft("Pasta", "basil"))
            .unwrap();
        assert_eq!(updated.created_at, recipe.created_at);
        assert_eq!(store.list_recipes(Some("tomato")).unwrap().len(), 0);
        store.delete_recipe(recipe.id).unwrap();
        assert!(matches!(
            store.get_recipe(recipe.id),
            Err(MealPlannerError::NotFound)
        ));
    }

    #[test]
    fn duplicate_titles_are_allowed_and_ordering_is_stable() {
        let store = store();
        let first = store.create_recipe(draft("Soup", "carrot")).unwrap();
        let second = store.create_recipe(draft("Soup", "potato")).unwrap();
        let results = store.list_recipes(None).unwrap();
        assert_eq!(
            results.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![first.id, second.id]
        );
    }
}
