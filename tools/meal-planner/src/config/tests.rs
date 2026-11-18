//! Tests for Meal Planner runtime configuration.

use std::path::PathBuf;

use crate::{MealPlannerConfig, MealPlannerError};

#[test]
fn default_database_is_inside_data_directory() {
    let config = MealPlannerConfig::from_data_dir("/var/lib/eme");

    assert_eq!(
        config.database_path(),
        PathBuf::from("/var/lib/eme/meal-planner.sqlite")
    );
}

#[test]
fn explicit_database_path_is_preserved() {
    let config = MealPlannerConfig::new("state/meals.sqlite").unwrap();

    assert_eq!(config.database_path(), PathBuf::from("state/meals.sqlite"));
}

#[test]
fn empty_database_path_is_rejected() {
    assert!(matches!(
        MealPlannerConfig::new(PathBuf::new()),
        Err(MealPlannerError::InvalidConfiguration(_))
    ));
}
