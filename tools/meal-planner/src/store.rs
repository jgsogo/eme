//! SQLite storage bootstrap and Meal Planner persistence boundary.
mod household;
#[cfg(test)]
mod household_tests;
mod migrations;
mod recipes;
#[cfg(test)]
mod tests;

use crate::{MealPlannerConfig, MealPlannerError};
use rusqlite::Connection;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

/// Shared SQLite connection for Meal Planner persistence operations.
#[derive(Clone)]
pub struct MealPlannerStore {
    pub(super) connection: Arc<Mutex<Connection>>,
}

impl MealPlannerStore {
    /// Open the configured database, creating its parent directory if necessary.
    pub fn open(config: &MealPlannerConfig) -> Result<Self, MealPlannerError> {
        let path = config.database_path();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        Self::from_connection(Connection::open(path)?)
    }

    /// Create an isolated in-memory database for tests.
    pub fn open_in_memory() -> Result<Self, MealPlannerError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    pub fn health_check(&self) -> Result<(), MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        connection.query_row("SELECT 1", [], |row| row.get::<_, i64>(0))?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64, MealPlannerError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MealPlannerError::LockPoisoned)?;
        Ok(connection.pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    fn from_connection(mut connection: Connection) -> Result<Self, MealPlannerError> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        migrations::apply(&mut connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }
}
