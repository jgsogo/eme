//! Tests for SQLite storage initialization and migrations.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::{MealPlannerConfig, MealPlannerError, MealPlannerStore};

#[test]
fn in_memory_database_is_initialized_and_healthy() {
    let store = MealPlannerStore::open_in_memory().unwrap();
    store.health_check().unwrap();
    assert_eq!(store.schema_version().unwrap(), 5);
}

#[test]
fn file_database_can_be_reopened_without_reapplying_migrations() {
    let path = temporary_database_path();
    let config = MealPlannerConfig::new(path.clone()).unwrap();
    {
        let store = MealPlannerStore::open(&config).unwrap();
        assert_eq!(store.schema_version().unwrap(), 4);
    }
    {
        let store = MealPlannerStore::open(&config).unwrap();
        store.health_check().unwrap();
        assert_eq!(store.schema_version().unwrap(), 4);
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn database_with_newer_schema_is_rejected() {
    let path = temporary_database_path();
    {
        let connection = Connection::open(&path).unwrap();
        connection.pragma_update(None, "user_version", 99).unwrap();
    }
    let config = MealPlannerConfig::new(path.clone()).unwrap();
    let error = match MealPlannerStore::open(&config) {
        Ok(_) => panic!("newer schema must not be opened"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        MealPlannerError::UnsupportedSchemaVersion {
            current: 99,
            supported: 5
        }
    ));
    let _ = std::fs::remove_file(path);
}

fn temporary_database_path() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "eme-meal-planner-{}-{nonce}.sqlite",
        std::process::id()
    ))
}
