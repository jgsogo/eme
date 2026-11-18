//! Provider-independent recipe domain types and deterministic validation.
use serde::{Deserialize, Serialize};

/// One structured ingredient. Unknown quantities remain `None`; no amount is inferred.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeIngredient {
    pub name: String,
    pub amount: Option<f64>,
    pub unit: Option<String>,
    pub preparation_note: Option<String>,
}

/// Input accepted when creating or replacing a recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeDraft {
    #[serde(default)]
    pub family_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub servings: u32,
    pub ingredients: Vec<RecipeIngredient>,
    /// Ordered preparation instructions; vector order is the stable step order.
    pub steps: Vec<String>,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
    pub source: Option<String>,
    pub notes: Option<String>,
    pub dietary_information: Vec<String>,
    /// None means unknown; Some([]) means explicitly checked and no allergens.
    pub allergens: Option<Vec<String>>,
}

/// Persisted recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub id: i64,
    pub owner_id: String,
    pub family_id: Option<String>,
    #[serde(flatten)]
    pub draft: RecipeDraft,
    pub created_at: i64,
    pub updated_at: i64,
}

impl RecipeDraft {
    pub(crate) fn validate(mut self) -> Result<Self, String> {
        let title = self.title.trim();
        if title.is_empty() {
            return Err("recipe title must not be empty".into());
        }
        if self.servings == 0 {
            return Err("servings must be greater than zero".into());
        }
        if self.ingredients.is_empty() {
            return Err("a recipe must contain at least one ingredient".into());
        }
        if self.steps.is_empty() || self.steps.iter().any(|step| step.trim().is_empty()) {
            return Err("a recipe must contain non-empty preparation steps".into());
        }
        for ingredient in &self.ingredients {
            if ingredient.name.trim().is_empty() {
                return Err("ingredient name must not be empty".into());
            }
            if ingredient
                .amount
                .is_some_and(|amount| !amount.is_finite() || amount <= 0.0)
            {
                return Err(format!(
                    "ingredient '{}' has an invalid amount",
                    ingredient.name
                ));
            }
            if ingredient
                .unit
                .as_ref()
                .is_some_and(|unit| unit.trim().is_empty())
            {
                return Err(format!(
                    "ingredient '{}' has an empty unit",
                    ingredient.name
                ));
            }
        }
        self.title = title.to_owned();
        for ingredient in &mut self.ingredients {
            ingredient.name = ingredient.name.trim().to_owned();
            ingredient.unit = ingredient.unit.take().map(|value| value.trim().to_owned());
            ingredient.preparation_note = ingredient
                .preparation_note
                .take()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty());
        }
        self.steps = self
            .steps
            .into_iter()
            .map(|step| step.trim().to_owned())
            .collect();
        self.categories = normalize_labels(std::mem::take(&mut self.categories));
        self.tags = normalize_labels(std::mem::take(&mut self.tags));
        self.dietary_information = normalize_labels(std::mem::take(&mut self.dietary_information));
        self.description = clean_optional(self.description.take());
        self.source = clean_optional(self.source.take());
        self.notes = clean_optional(self.notes.take());
        self.allergens = self.allergens.take().map(normalize_labels);
        Ok(self)
    }
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn normalize_labels(values: Vec<String>) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    for value in values {
        let value = value.trim();
        if !value.is_empty()
            && !result
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(value))
        {
            result.push(value.to_owned());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> RecipeDraft {
        RecipeDraft {
            family_id: None,
            title: "  Tomato pasta ".into(),
            description: None,
            servings: 2,
            ingredients: vec![RecipeIngredient {
                name: " Pasta ".into(),
                amount: None,
                unit: None,
                preparation_note: None,
            }],
            steps: vec!["  Boil water ".into()],
            categories: vec![" Dinner ".into(), "dinner".into()],
            tags: vec![],
            source: None,
            notes: None,
            dietary_information: vec![],
            allergens: None,
        }
    }

    #[test]
    fn validation_normalizes_text_but_preserves_unknown_quantities_and_allergens() {
        let recipe = draft().validate().unwrap();
        assert_eq!(recipe.title, "Tomato pasta");
        assert_eq!(recipe.ingredients[0].name, "Pasta");
        assert_eq!(recipe.ingredients[0].amount, None);
        assert_eq!(recipe.categories, ["Dinner"]);
        assert_eq!(recipe.allergens, None);
    }

    #[test]
    fn rejects_zero_servings_empty_steps_and_invalid_amounts() {
        let mut value = draft();
        value.servings = 0;
        assert!(value.validate().is_err());
        let mut value = draft();
        value.steps.clear();
        assert!(value.validate().is_err());
        let mut value = draft();
        value.ingredients[0].amount = Some(f64::NAN);
        assert!(value.validate().is_err());
    }

    #[test]
    fn empty_known_allergen_list_is_not_unknown() {
        let mut value = draft();
        value.allergens = Some(vec![]);
        assert_eq!(value.validate().unwrap().allergens, Some(vec![]));
    }
}
