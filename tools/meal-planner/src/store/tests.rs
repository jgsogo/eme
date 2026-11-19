//! Tests for SQLite storage initialization and migrations.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::{MealPlannerConfig, MealPlannerError, MealPlannerStore};

#[test]
fn in_memory_database_is_initialized_and_healthy() {
    let store = MealPlannerStore::open_in_memory().unwrap();
    store.health_check().unwrap();
    assert_eq!(store.schema_version().unwrap(), 10);
}

#[test]
fn file_database_can_be_reopened_without_reapplying_migrations() {
    let path = temporary_database_path();
    let config = MealPlannerConfig::new(path.clone()).unwrap();
    {
        let store = MealPlannerStore::open(&config).unwrap();
        assert_eq!(store.schema_version().unwrap(), 10);
    }
    {
        let store = MealPlannerStore::open(&config).unwrap();
        store.health_check().unwrap();
        assert_eq!(store.schema_version().unwrap(), 9);
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
            supported: 10
        }
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn migration_10_removes_breakfast_participation_and_preserves_other_meals() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch(
        r#"
        CREATE TABLE family_members (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            family_id TEXT NOT NULL,
            external_id TEXT NOT NULL,
            name TEXT NOT NULL,
            active INTEGER NOT NULL DEFAULT 1,
            UNIQUE(family_id, external_id)
        );
        CREATE TABLE recurring_participation_rules (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            family_id TEXT NOT NULL,
            member_id INTEGER NOT NULL REFERENCES family_members(id) ON DELETE RESTRICT,
            weekday INTEGER NOT NULL CHECK(weekday BETWEEN 0 AND 6),
            meal_type TEXT NOT NULL,
            arrangement TEXT NOT NULL,
            effective_from TEXT,
            effective_until TEXT
        );
        CREATE TABLE meal_participation_exceptions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            family_id TEXT NOT NULL,
            member_id INTEGER NOT NULL REFERENCES family_members(id) ON DELETE RESTRICT,
            date TEXT NOT NULL,
            meal_type TEXT NOT NULL,
            arrangement TEXT NOT NULL,
            UNIQUE(family_id, member_id, date, meal_type)
        );
        INSERT INTO family_members(id, family_id, external_id, name) VALUES (1, 'family-a', 'user-1', 'Alex');
        INSERT INTO recurring_participation_rules
            (family_id, member_id, weekday, meal_type, arrangement)
        VALUES
            ('family-a', 1, 0, 'breakfast', 'home'),
            ('family-a', 1, 0, 'lunch', 'school'),
            ('family-a', 1, 0, 'dinner', 'home');
        INSERT INTO meal_participation_exceptions
            (family_id, member_id, date, meal_type, arrangement)
        VALUES
            ('family-a', 1, '2026-11-16', 'breakfast', 'home'),
            ('family-a', 1, '2026-11-16', 'lunch', 'away'),
            ('family-a', 1, '2026-11-16', 'dinner', 'home');
        PRAGMA user_version = 9;
        "#,
    ).unwrap();

    let store = MealPlannerStore::from_connection(connection).unwrap();
    assert_eq!(store.schema_version().unwrap(), 10);
    let connection = store.connection.lock().unwrap();
    let rules: Vec<String> = {
        let mut statement = connection.prepare(
            "SELECT meal_type FROM recurring_participation_rules ORDER BY meal_type"
        ).unwrap();
        statement.query_map([], |row| row.get(0)).unwrap()
            .collect::<Result<_, _>>().unwrap()
    };
    assert_eq!(rules, vec!["dinner".to_owned(), "lunch".to_owned()]);
    let exceptions: Vec<String> = {
        let mut statement = connection.prepare(
            "SELECT meal_type FROM meal_participation_exceptions ORDER BY meal_type"
        ).unwrap();
        statement.query_map([], |row| row.get(0)).unwrap()
            .collect::<Result<_, _>>().unwrap()
    };
    assert_eq!(exceptions, vec!["dinner".to_owned(), "lunch".to_owned()]);
    assert!(connection.execute(
        "INSERT INTO recurring_participation_rules (family_id, member_id, weekday, meal_type, arrangement) VALUES ('family-a', 1, 0, 'breakfast', 'home')",
        []
    ).is_err());
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

#[test]
fn migration_9_keeps_latest_plan_and_its_shopping_list_and_preserves_school_menu_data() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE weekly_plans (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                family_id TEXT NOT NULL,
                week_start TEXT NOT NULL,
                revision INTEGER NOT NULL,
                status TEXT NOT NULL,
                assignments TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                confirmed_at INTEGER,
                UNIQUE(family_id, week_start, revision)
            );
            CREATE INDEX idx_weekly_plans_family_week ON weekly_plans(family_id, week_start);
            CREATE TABLE shopping_lists (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                plan_id INTEGER NOT NULL UNIQUE REFERENCES weekly_plans(id) ON DELETE CASCADE,
                week_start TEXT NOT NULL,
                plan_revision INTEGER NOT NULL,
                items TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX idx_shopping_lists_week ON shopping_lists(week_start);
            CREATE TABLE school_menus (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                family_id TEXT NOT NULL,
                date TEXT NOT NULL,
                status TEXT NOT NULL,
                payload TEXT NOT NULL,
                source TEXT,
                imported_at INTEGER,
                origin TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE(family_id, date)
            );
            CREATE INDEX idx_school_menus_family_date ON school_menus(family_id, date);

            INSERT INTO weekly_plans
                (id, family_id, week_start, revision, status, assignments, created_at, confirmed_at)
            VALUES
                (10, 'family-a', '2026-11-16', 1, 'draft', '[]', 100, NULL),
                (11, 'family-a', '2026-11-16', 2, 'confirmed', '[{"id":2}]', 200, 210),
                (20, 'family-b', '2026-11-16', 1, 'confirmed', '[{"id":3}]', 300, 310);

            INSERT INTO shopping_lists
                (id, plan_id, week_start, plan_revision, items, created_at, updated_at)
            VALUES
                (100, 10, '2026-11-16', 1, '[{"name":"old revision item"}]', 101, 102),
                (110, 11, '2026-11-16', 2, '[{"name":"latest item","purchased":true,"manually_removed":true}]', 201, 202),
                (200, 20, '2026-11-16', 1, '[{"name":"other family item"}]', 301, 302);

            INSERT INTO school_menus
                (id, family_id, date, status, payload, source, imported_at, origin, created_at, updated_at)
            VALUES
                (7, 'family-a', '2026-11-17', 'known', '{"lunch":"Pasta"}', 'school-site', 1234, 'import', 100, 200);

            PRAGMA user_version = 8;
            "#,
        )
        .unwrap();

    let store = MealPlannerStore::from_connection(connection).unwrap();
    assert_eq!(store.schema_version().unwrap(), 9);

    let connection = store.connection.lock().unwrap();
    let plans: Vec<(i64, String)> = {
        let mut statement = connection
            .prepare("SELECT id, assignments FROM weekly_plans ORDER BY id")
            .unwrap();
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(
        plans,
        vec![
            (11, r#"[{"id":2}]"#.to_owned()),
            (20, r#"[{"id":3}]"#.to_owned())
        ]
    );

    let lists: Vec<(i64, String)> = {
        let mut statement = connection
            .prepare("SELECT id, items FROM shopping_lists ORDER BY id")
            .unwrap();
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(
        lists,
        vec![
            (
                110,
                r#"[{"name":"latest item","purchased":true,"manually_removed":true}]"#.to_owned()
            ),
            (200, r#"[{"name":"other family item"}]"#.to_owned())
        ]
    );

    let school_menu: (String, String) = connection
        .query_row(
            "SELECT date, payload FROM school_menus WHERE id=7",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        school_menu,
        ("2026-11-17".into(), r#"{"lunch":"Pasta"}"#.into())
    );

    let school_columns: Vec<String> = {
        let mut statement = connection
            .prepare("PRAGMA table_info(school_menus)")
            .unwrap();
        statement
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert!(!school_columns.iter().any(|name| name == "source"));
    assert!(!school_columns.iter().any(|name| name == "imported_at"));
    assert!(!school_columns.iter().any(|name| name == "origin"));
}
