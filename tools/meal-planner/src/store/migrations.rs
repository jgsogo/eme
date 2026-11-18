//! Ordered, transactional SQLite schema migrations.
use crate::MealPlannerError;
use rusqlite::Connection;

struct Migration {
    version: i64,
    sql: &'static str,
}
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("../../migrations/0001_initial.sql"),
    },
    Migration {
        version: 2,
        sql: include_str!("../../migrations/0002_household.sql"),
    },
    Migration {
        version: 3,
        sql: include_str!("../../migrations/0003_recipes.sql"),
    },
    Migration {
        version: 4,
        sql: include_str!("../../migrations/0004_school_menus.sql"),
    },
];
const LATEST_SCHEMA_VERSION: i64 = 4;

pub(super) fn apply(connection: &mut Connection) -> Result<(), MealPlannerError> {
    let current: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current > LATEST_SCHEMA_VERSION {
        return Err(MealPlannerError::UnsupportedSchemaVersion {
            current,
            supported: LATEST_SCHEMA_VERSION,
        });
    }
    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version > current)
    {
        let transaction = connection.transaction()?;
        transaction.execute_batch(migration.sql)?;
        transaction.pragma_update(None, "user_version", migration.version)?;
        transaction.commit()?;
    }
    Ok(())
}
