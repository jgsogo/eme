//! Persistence tests for household configuration and resolution.
use crate::{
    MealArrangement, MealPlannerStore, MealType, MealWeekday, ParticipationSource,
    ParticipationStatus,
};
use chrono::NaiveDate;
fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

#[test]
fn member_lifecycle_and_referential_constraints() {
    let store = MealPlannerStore::open_in_memory().unwrap();
    let member = store.create_family_member(" Alex ").unwrap();
    assert_eq!(member.name, "Alex");
    store
        .add_recurring_rule(
            member.id,
            MealWeekday::Monday,
            MealType::Lunch,
            MealArrangement::Home,
            None,
            None,
        )
        .unwrap();
    assert!(store.delete_family_member(member.id).is_err());
    store.deactivate_family_member(member.id).unwrap();
    assert!(!store.list_family_members().unwrap()[0].active);
    store
        .rename_family_member(member.id, "Alex Rivera")
        .unwrap();
}

#[test]
fn recurring_rule_and_date_exception_resolve_week() {
    let store = MealPlannerStore::open_in_memory().unwrap();
    let member = store.create_family_member("Alex").unwrap();
    store
        .add_recurring_rule(
            member.id,
            MealWeekday::Monday,
            MealType::Lunch,
            MealArrangement::School,
            None,
            None,
        )
        .unwrap();
    store
        .set_meal_exception(
            member.id,
            date("2026-11-17"),
            MealType::Lunch,
            MealArrangement::Away,
        )
        .unwrap();
    let week = store.resolve_meal_week(date("2026-11-16")).unwrap();
    let monday = week
        .iter()
        .find(|v| v.date == date("2026-11-16") && v.meal_type == MealType::Lunch)
        .unwrap();
    let tuesday = week
        .iter()
        .find(|v| v.date == date("2026-11-17") && v.meal_type == MealType::Lunch)
        .unwrap();
    assert_eq!(
        monday.status,
        ParticipationStatus::Configured {
            arrangement: MealArrangement::School,
            source: ParticipationSource::RecurringRule
        }
    );
    assert_eq!(
        tuesday.status,
        ParticipationStatus::Configured {
            arrangement: MealArrangement::Away,
            source: ParticipationSource::DateException
        }
    );
}

#[test]
fn missing_rule_differs_from_explicit_not_participating() {
    let store = MealPlannerStore::open_in_memory().unwrap();
    let member = store.create_family_member("Alex").unwrap();
    let date = date("2026-11-16");
    assert_eq!(
        store.resolve_meal_date(date).unwrap()[0].status,
        ParticipationStatus::Unconfigured
    );
    store
        .set_meal_exception(
            member.id,
            date,
            MealType::Lunch,
            MealArrangement::NotParticipating,
        )
        .unwrap();
    assert_eq!(
        store.resolve_meal_date(date).unwrap()[0].status,
        ParticipationStatus::Configured {
            arrangement: MealArrangement::NotParticipating,
            source: ParticipationSource::DateException
        }
    );
}

#[test]
fn invalid_values_are_rejected() {
    let store = MealPlannerStore::open_in_memory().unwrap();
    assert!(store.create_family_member(" ").is_err());
    let member = store.create_family_member("Alex").unwrap();
    assert!(
        store
            .add_recurring_rule(
                member.id,
                MealWeekday::Monday,
                MealType::Lunch,
                MealArrangement::Home,
                Some(date("2026-11-20")),
                Some(date("2026-11-16"))
            )
            .is_err()
    );
}

#[test]
fn recurring_rule_writes_are_idempotent_for_the_same_schedule() {
    let store = MealPlannerStore::open_in_memory().unwrap();
    let members = store
        .sync_family_members("family-a", &[("user-1".into(), "Alex".into())])
        .unwrap();
    let member_id = members[0].id;

    let first = store
        .add_recurring_rule_for_family(
            "family-a",
            member_id,
            MealWeekday::Monday,
            MealType::Lunch,
            MealArrangement::Home,
            None,
            None,
        )
        .unwrap();
    let second = store
        .add_recurring_rule_for_family(
            "family-a",
            member_id,
            MealWeekday::Monday,
            MealType::Lunch,
            MealArrangement::School,
            None,
            None,
        )
        .unwrap();

    assert_eq!(first.id, second.id);
    let monday = store
        .resolve_meal_week_for_family("family-a", date("2026-11-16"))
        .unwrap()
        .into_iter()
        .find(|item| item.date == date("2026-11-16") && item.meal_type == MealType::Lunch)
        .unwrap();
    assert_eq!(
        monday.status,
        ParticipationStatus::Configured {
            arrangement: MealArrangement::School,
            source: ParticipationSource::RecurringRule,
        }
    );
}
