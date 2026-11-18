//! Runtime configuration for the Meal Planner module.

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use crate::MealPlannerError;

/// Configuration shared by the Meal Planner application and its adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MealPlannerConfig {
    database_path: PathBuf,
}

impl MealPlannerConfig {
    /// Build configuration using the default database location in the server data directory.
    pub fn from_data_dir(data_dir: impl AsRef<Path>) -> Self {
        Self {
            database_path: data_dir.as_ref().join("meal-planner.sqlite"),
        }
    }

    /// Build configuration with an explicit SQLite database path.
    pub fn new(database_path: impl Into<PathBuf>) -> Result<Self, MealPlannerError> {
        let database_path = database_path.into();
        if database_path.as_os_str().is_empty() {
            return Err(MealPlannerError::InvalidConfiguration(
                "database path must not be empty".into(),
            ));
        }
        Ok(Self { database_path })
    }

    /// Return the configured SQLite database path.
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }
}
