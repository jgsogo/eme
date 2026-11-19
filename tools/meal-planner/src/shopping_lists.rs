//! Deterministic shopping-list generation and reconciliation.
use crate::{MealArrangement, WeeklyPlan};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShoppingList {
    pub id: i64,
    pub plan_id: i64,
    pub plan_revision: i64,
    pub items: Vec<ShoppingListItem>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShoppingListItem {
    pub id: i64,
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub purchased: bool,
    pub source_assignment_ids: Vec<i64>,
    pub source_recipe_ids: Vec<i64>,
    /// Manually added entries are retained verbatim during regeneration.
    pub manually_added: bool,
    /// A removed generated entry is retained as a tombstone across regeneration.
    pub manually_removed: bool,
    /// User quantity edits survive subsequent generation.
    pub quantity_overridden: bool,
    /// A source assignment disappeared from the selected plan revision.
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewShoppingListItem {
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShoppingListItemUpdate {
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub purchased: bool,
}

#[derive(Debug, Clone)]
pub(super) struct GeneratedItem {
    name: String,
    quantity: Option<f64>,
    unit: Option<String>,
    source_assignment_ids: Vec<i64>,
    source_recipe_ids: Vec<i64>,
}

/// Build the generated portion of a list. Participant count is the planned
/// serving count for an assignment; recipe servings are its base serving count.
pub(super) fn generate_items(
    plan: &WeeklyPlan,
    mut recipe_for: impl FnMut(i64) -> Option<crate::Recipe>,
) -> Result<Vec<GeneratedItem>, String> {
    let mut items: Vec<GeneratedItem> = Vec::new();
    for assignment in &plan.assignments {
        for recipe_id in &assignment.recipe_ids {
            let recipe_id = *recipe_id;
            let recipe = recipe_for(recipe_id)
                .ok_or_else(|| format!("recipe {recipe_id} referenced by plan was not found"))?;
            let factor = assignment.participants.len() as f64 / recipe.draft.servings as f64;
            for ingredient in recipe.draft.ingredients {
                let (unit, multiplier) = normalize_unit(ingredient.unit.as_deref());
                let quantity = ingredient.amount.map(|amount| amount * factor * multiplier);
                let key = (normalize_name(&ingredient.name), unit.clone());
                let existing = unit.as_ref().and_then(|_| {
                    items
                        .iter_mut()
                        .find(|item| (normalize_name(&item.name), item.unit.clone()) == key)
                });
                if let Some(existing) = existing {
                    existing.quantity = match (existing.quantity, quantity) {
                        (Some(left), Some(right)) => Some(left + right),
                        _ => None,
                    };
                    push_unique(&mut existing.source_assignment_ids, assignment.id);
                    push_unique(&mut existing.source_recipe_ids, recipe_id);
                } else {
                    items.push(GeneratedItem {
                        name: ingredient.name,
                        quantity,
                        unit,
                        source_assignment_ids: vec![assignment.id],
                        source_recipe_ids: vec![recipe_id],
                    });
                }
            }
        }
    }
    Ok(items)
}

fn normalize_name(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Convert only explicitly supported units. Unknown units are grouped only
/// with the same normalized unit; missing units remain distinct from all units.
fn normalize_unit(value: Option<&str>) -> (Option<String>, f64) {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return (None, 1.0);
    };
    match value.to_ascii_lowercase().as_str() {
        "g" | "gram" | "grams" => (Some("g".into()), 1.0),
        "kg" | "kilogram" | "kilograms" => (Some("g".into()), 1000.0),
        "ml" | "milliliter" | "milliliters" | "millilitre" | "millilitres" => {
            (Some("ml".into()), 1.0)
        }
        "l" | "liter" | "liters" | "litre" | "litres" => (Some("ml".into()), 1000.0),
        "pc" | "pcs" | "piece" | "pieces" | "unit" | "units" => (Some("piece".into()), 1.0),
        other => (Some(other.to_owned()), 1.0),
    }
}

fn push_unique(values: &mut Vec<i64>, value: i64) {
    if !values.contains(&value) {
        values.push(value);
    }
}

impl ShoppingList {
    pub(super) fn reconcile(
        existing: Option<Self>,
        plan_id: i64,
        plan_revision: i64,
        generated: Vec<GeneratedItem>,
        now: i64,
    ) -> Self {
        let (id, created_at, mut old_items) = existing
            .map(|list| (list.id, list.created_at, list.items))
            .unwrap_or((0, now, Vec::new()));
        let mut next_id = old_items.iter().map(|item| item.id).max().unwrap_or(0) + 1;
        let mut items = Vec::new();

        // Keep manual entries as-is and match generated entries by ingredient + normalized unit.
        for generated_item in generated {
            let match_index = old_items.iter().position(|item| {
                !item.manually_added
                    && normalize_name(&item.name) == normalize_name(&generated_item.name)
                    && (item.manually_removed
                        || item.quantity_overridden
                        || normalize_unit(item.unit.as_deref()).0
                            == normalize_unit(generated_item.unit.as_deref()).0)
            });
            if let Some(index) = match_index {
                let mut previous = old_items.remove(index);
                previous.source_assignment_ids = generated_item.source_assignment_ids;
                previous.source_recipe_ids = generated_item.source_recipe_ids;
                previous.stale = false;
                if !previous.quantity_overridden {
                    previous.quantity = generated_item.quantity;
                    previous.unit = generated_item.unit;
                }
                items.push(previous);
            } else {
                items.push(ShoppingListItem {
                    id: next_id,
                    name: generated_item.name,
                    quantity: generated_item.quantity,
                    unit: generated_item.unit,
                    purchased: false,
                    source_assignment_ids: generated_item.source_assignment_ids,
                    source_recipe_ids: generated_item.source_recipe_ids,
                    manually_added: false,
                    manually_removed: false,
                    quantity_overridden: false,
                    stale: false,
                });
                next_id += 1;
            }
        }

        // Preserve manually added items and removed/edited/purchased entries whose
        // source no longer appears; mark generated leftovers stale for review.
        for mut previous in old_items {
            if previous.manually_added
                || previous.manually_removed
                || previous.purchased
                || previous.quantity_overridden
            {
                if !previous.manually_added {
                    previous.stale = true;
                    previous.source_assignment_ids.clear();
                    previous.source_recipe_ids.clear();
                }
                items.push(previous);
            }
        }
        items.sort_by(|a, b| {
            a.manually_added
                .cmp(&b.manually_added)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.id.cmp(&b.id))
        });
        Self {
            id,
            plan_id,
            plan_revision,
            items,
            created_at,
            updated_at: now,
        }
    }
}

impl NewShoppingListItem {
    pub(crate) fn validate(self) -> Result<Self, String> {
        if self.name.trim().is_empty() {
            return Err("shopping-list item name must not be empty".into());
        }
        if self
            .quantity
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
        {
            return Err("shopping-list quantity must be a positive finite number".into());
        }
        if self
            .unit
            .as_ref()
            .is_some_and(|unit| unit.trim().is_empty())
        {
            return Err("shopping-list unit must not be empty".into());
        }
        Ok(Self {
            name: self.name.trim().to_owned(),
            quantity: self.quantity,
            unit: self.unit.map(|value| value.trim().to_owned()),
        })
    }
}

impl ShoppingListItemUpdate {
    pub(crate) fn validate(self) -> Result<Self, String> {
        if self.name.trim().is_empty() {
            return Err("shopping-list item name must not be empty".into());
        }
        if self
            .quantity
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
        {
            return Err("shopping-list quantity must be a positive finite number".into());
        }
        if self
            .unit
            .as_ref()
            .is_some_and(|unit| unit.trim().is_empty())
        {
            return Err("shopping-list unit must not be empty".into());
        }
        Ok(Self {
            name: self.name.trim().to_owned(),
            quantity: self.quantity,
            unit: self.unit.map(|value| value.trim().to_owned()),
            purchased: self.purchased,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MealAssignment, MealType, PlanParticipant, RecipeDraft, RecipeIngredient, WeeklyPlanStatus,
    };
    use chrono::NaiveDate;

    fn recipe(id: i64, servings: u32, amount: Option<f64>, unit: Option<&str>) -> crate::Recipe {
        crate::Recipe {
            id,
            owner_id: "test-user".into(),
            draft: RecipeDraft {
                family_id: None,
                title: format!("Recipe {id}"),
                description: None,
                servings,
                ingredients: vec![RecipeIngredient {
                    name: "Tomato".into(),
                    amount,
                    unit: unit.map(str::to_owned),
                    preparation_note: None,
                }],
                steps: vec!["Cook".into()],
                categories: vec![],
                tags: vec![],
                source: None,
                notes: None,
                dietary_information: vec![],
                allergens: None,
            },
            created_at: 0,
            updated_at: 0,
        }
    }

    fn plan(assignments: Vec<MealAssignment>) -> WeeklyPlan {
        WeeklyPlan {
            id: 7,
            week_start: NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(),
            revision: 2,
            status: WeeklyPlanStatus::Draft,
            assignments,
            created_at: 0,
            confirmed_at: None,
        }
    }

    fn assignment(
        id: i64,
        arrangement: MealArrangement,
        recipe_ids: Vec<i64>,
        participants: usize,
    ) -> MealAssignment {
        MealAssignment {
            id,
            date: NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(),
            meal_type: MealType::Lunch,
            participants: (1..=participants)
                .map(|member_id| PlanParticipant {
                    member_id: member_id as i64,
                    name: format!("Member {member_id}"),
                })
                .collect(),
            recipe_ids,
        }
    }

    #[test]
    fn scales_by_participant_count_and_converts_compatible_units() {
        let p = plan(vec![
            assignment(1, MealArrangement::Home, vec![1], 4),
            assignment(2, MealArrangement::Home, vec![2], 1),
        ]);
        let generated = generate_items(&p, |id| match id {
            1 => Some(recipe(1, 2, Some(250.0), Some("g"))),
            2 => Some(recipe(2, 1, Some(0.25), Some("kg"))),
            _ => None,
        })
        .unwrap();
        assert_eq!(generated.len(), 1);
        assert_eq!(generated[0].quantity, Some(750.0));
        assert_eq!(generated[0].unit.as_deref(), Some("g"));
        assert_eq!(generated[0].source_assignment_ids, [1, 2]);
    }

    #[test]
    fn incompatible_or_missing_units_are_not_aggregated() {
        let p = plan(vec![
            assignment(1, MealArrangement::Home, vec![1], 1),
            assignment(2, MealArrangement::Home, vec![2], 1),
            assignment(3, MealArrangement::Home, vec![3], 1),
            assignment(4, MealArrangement::Home, vec![4], 1),
        ]);
        let generated = generate_items(&p, |id| match id {
            1 => Some(recipe(1, 1, Some(100.0), Some("g"))),
            2 => Some(recipe(2, 1, Some(100.0), Some("ml"))),
            3 => Some(recipe(3, 1, Some(2.0), None)),
            4 => Some(recipe(4, 1, Some(3.0), None)),
            _ => None,
        })
        .unwrap();
        assert_eq!(generated.len(), 4);
        assert_eq!(generated[0].unit.as_deref(), Some("g"));
        assert_eq!(generated[1].unit.as_deref(), Some("ml"));
        assert_eq!(generated[2].quantity, Some(2.0));
        assert_eq!(generated[3].quantity, Some(3.0));
        assert!(generated[2].unit.is_none());
        assert!(generated[3].unit.is_none());
    }

    #[test]
    fn unknown_quantities_remain_unknown_and_assignments_without_recipes_are_ignored() {
        let p = plan(vec![
            assignment(1, MealArrangement::Home, vec![1], 2),
            assignment(2, MealArrangement::School, vec![], 1),
            assignment(3, MealArrangement::Away, vec![], 1),
        ]);
        let generated =
            generate_items(&p, |id| (id == 1).then(|| recipe(1, 2, None, None))).unwrap();
        assert_eq!(generated.len(), 1);
        assert_eq!(generated[0].quantity, None);
        assert_eq!(generated[0].unit, None);
    }

    #[test]
    fn reconciliation_preserves_manual_additions_removals_purchased_and_overrides() {
        let old = ShoppingList {
            id: 9,
            plan_id: 7,
            plan_revision: 1,
            created_at: 1,
            updated_at: 2,
            items: vec![
                ShoppingListItem {
                    id: 1,
                    name: "Tomato".into(),
                    quantity: Some(2.0),
                    unit: Some("piece".into()),
                    purchased: true,
                    source_assignment_ids: vec![1],
                    source_recipe_ids: vec![1],
                    manually_added: false,
                    manually_removed: false,
                    quantity_overridden: true,
                    stale: false,
                },
                ShoppingListItem {
                    id: 2,
                    name: "Bread".into(),
                    quantity: None,
                    unit: None,
                    purchased: false,
                    source_assignment_ids: vec![],
                    source_recipe_ids: vec![],
                    manually_added: true,
                    manually_removed: false,
                    quantity_overridden: false,
                    stale: false,
                },
                ShoppingListItem {
                    id: 3,
                    name: "Milk".into(),
                    quantity: None,
                    unit: None,
                    purchased: false,
                    source_assignment_ids: vec![],
                    source_recipe_ids: vec![],
                    manually_added: false,
                    manually_removed: true,
                    quantity_overridden: false,
                    stale: false,
                },
            ],
        };
        let generated = vec![
            GeneratedItem {
                name: "Tomato".into(),
                quantity: Some(4.0),
                unit: Some("piece".into()),
                source_assignment_ids: vec![2],
                source_recipe_ids: vec![2],
            },
            GeneratedItem {
                name: "Milk".into(),
                quantity: Some(1.0),
                unit: Some("l".into()),
                source_assignment_ids: vec![3],
                source_recipe_ids: vec![3],
            },
        ];
        let result = ShoppingList::reconcile(Some(old), 7, 2, generated, 3);
        assert_eq!(result.id, 9);
        assert_eq!(result.plan_revision, 2);
        let tomato = result
            .items
            .iter()
            .find(|item| item.name == "Tomato")
            .unwrap();
        assert_eq!(tomato.quantity, Some(2.0));
        assert!(tomato.purchased);
        let bread = result
            .items
            .iter()
            .find(|item| item.name == "Bread")
            .unwrap();
        assert!(bread.manually_added);
        let milk = result
            .items
            .iter()
            .find(|item| item.name == "Milk")
            .unwrap();
        assert!(milk.manually_removed);
    }
}
