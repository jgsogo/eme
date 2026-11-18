//! SQLite persistence operations for the recipe catalogue.
use crate::{MealPlannerError, MealPlannerStore, Recipe, RecipeDraft};
use rusqlite::{OptionalExtension, params};
use std::time::{SystemTime, UNIX_EPOCH};

impl MealPlannerStore {
    pub fn create_recipe(
        &self,
        owner_id: &str,
        draft: RecipeDraft,
    ) -> Result<Recipe, MealPlannerError> {
        let draft = draft.validate().map_err(MealPlannerError::InvalidInput)?;
        let payload = serde_json::to_string(&draft)?;
        let now = timestamp();
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO recipes(title,payload,owner_id,family_id,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?5)",
            params![draft.title, payload, owner_id, draft.family_id, now],
        )?;
        let id = transaction.last_insert_rowid();
        transaction.commit()?;
        Ok(Recipe {
            id,
            owner_id: owner_id.to_owned(),
            draft,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn update_recipe(
        &self,
        id: i64,
        owner_id: &str,
        draft: RecipeDraft,
    ) -> Result<Recipe, MealPlannerError> {
        let draft = draft.validate().map_err(MealPlannerError::InvalidInput)?;
        let payload = serde_json::to_string(&draft)?;
        let now = timestamp();
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        if connection.execute(
            "UPDATE recipes SET title=?1,payload=?2,family_id=?3,updated_at=?4 WHERE id=?5 AND owner_id=?6",
            params![draft.title, payload, draft.family_id, now, id, owner_id],
        )? == 0
        {
            return Err(MealPlannerError::NotFound);
        }
        load_recipe(&connection, id)?.ok_or(MealPlannerError::NotFound)
    }

    pub fn get_recipe(
        &self,
        id: i64,
        owner_id: &str,
        families: &[String],
    ) -> Result<Recipe, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let recipe = load_recipe(&connection, id)?.ok_or(MealPlannerError::NotFound)?;
        if recipe.owner_id != owner_id
            && !recipe
                .draft
                .family_id
                .as_ref()
                .is_some_and(|family| families.iter().any(|available| available == family))
        {
            return Err(MealPlannerError::NotFound);
        }
        Ok(recipe)
    }

    pub fn list_recipes(
        &self,
        owner_id: &str,
        families: &[String],
        query: Option<&str>,
    ) -> Result<Vec<Recipe>, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT id,payload,owner_id,family_id,created_at,updated_at FROM recipes ORDER BY lower(title),id",
        )?;
        let rows = statement
            .query_map([], row_recipe)?
            .collect::<Result<Vec<_>, _>>()?;
        let needle = query
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_lowercase);

        Ok(rows
            .into_iter()
            .filter(|recipe| {
                (recipe.owner_id == owner_id
                    || recipe
                        .draft
                        .family_id
                        .as_ref()
                        .is_some_and(|family| families.iter().any(|available| available == family)))
                    && needle.as_ref().is_none_or(|needle| {
                        let draft = &recipe.draft;
                        draft.title.to_lowercase().contains(needle)
                            || draft
                                .description
                                .as_deref()
                                .is_some_and(|value| value.to_lowercase().contains(needle))
                            || draft
                                .ingredients
                                .iter()
                                .any(|ingredient| ingredient.name.to_lowercase().contains(needle))
                            || draft
                                .tags
                                .iter()
                                .chain(&draft.categories)
                                .any(|value| value.to_lowercase().contains(needle))
                    })
            })
            .collect())
    }

    pub fn delete_recipe(&self, id: i64, owner_id: &str) -> Result<(), MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        if connection.execute(
            "DELETE FROM recipes WHERE id=?1 AND owner_id=?2",
            params![id, owner_id],
        )? == 0
        {
            return Err(MealPlannerError::NotFound);
        }
        Ok(())
    }
}

fn row_recipe(row: &rusqlite::Row<'_>) -> rusqlite::Result<Recipe> {
    let payload: String = row.get(1)?;
    let draft = serde_json::from_str(&payload).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(Recipe {
        id: row.get(0)?,
        draft,
        owner_id: row.get(2)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

pub(super) fn load_recipe(
    connection: &rusqlite::Connection,
    id: i64,
) -> Result<Option<Recipe>, MealPlannerError> {
    Ok(connection
        .query_row(
            "SELECT id,payload,owner_id,family_id,created_at,updated_at FROM recipes WHERE id=?1",
            [id],
            row_recipe,
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

    fn draft(title: &str, ingredient: &str, family_id: Option<&str>) -> RecipeDraft {
        RecipeDraft {
            family_id: family_id.map(str::to_owned),
            title: title.into(),
            description: None,
            servings: 2,
            ingredients: vec![RecipeIngredient {
                name: ingredient.into(),
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
        }
    }

    #[test]
    fn access_is_owner_or_matching_family() {
        let store = store();
        let private = store
            .create_recipe("a", draft("Private", "x", None))
            .unwrap();
        let shared = store
            .create_recipe("a", draft("Shared", "y", Some("f1")))
            .unwrap();
        store
            .create_recipe("b", draft("Other", "z", Some("f2")))
            .unwrap();

        assert_eq!(store.list_recipes("a", &[], None).unwrap().len(), 2);
        assert!(store.get_recipe(private.id, "b", &[]).is_err());
        assert!(store.get_recipe(shared.id, "b", &["f1".into()]).is_ok());
    }

    #[test]
    fn only_owner_can_mutate() {
        let store = store();
        let recipe = store
            .create_recipe("a", draft("Soup", "x", Some("f1")))
            .unwrap();

        assert!(
            store
                .update_recipe(recipe.id, "b", draft("No", "x", None))
                .is_err()
        );
        assert_eq!(
            store
                .update_recipe(recipe.id, "a", draft("Soup", "x", None))
                .unwrap()
                .draft
                .family_id,
            None
        );
        assert!(store.delete_recipe(recipe.id, "b").is_err());
        store.delete_recipe(recipe.id, "a").unwrap();
    }
}
