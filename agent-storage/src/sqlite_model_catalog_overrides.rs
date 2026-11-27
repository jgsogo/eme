//! SQLite persistence for sparse provider/model catalogue overrides.

use std::{path::Path, sync::Arc};

use agent_models::{AccessTier, ModelCatalogOverrides, ModelOverride, ProviderOverride};
use async_trait::async_trait;
use rusqlite::{Connection, params};
use tokio::sync::Mutex;

use crate::{ModelCatalogOverrideStore, StorageError};

#[derive(Clone)]
pub struct SqliteModelCatalogOverrideStore {
    connection: Arc<Mutex<Connection>>,
}

impl SqliteModelCatalogOverrideStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let connection = Connection::open(path).map_err(backend)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(backend)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(backend)?;
        connection
            .execute_batch(include_str!(
                "../migrations/0010_model_catalog_overrides.sql"
            ))
            .map_err(backend)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }
}

fn backend(error: rusqlite::Error) -> StorageError {
    StorageError::Backend(error.to_string())
}

fn access_tier_name(value: AccessTier) -> &'static str {
    match value {
        AccessTier::Free => "free",
        AccessTier::FreeTier => "free_tier",
        AccessTier::Trial => "trial",
        AccessTier::Paid => "paid",
        AccessTier::Unknown => "unknown",
    }
}

fn parse_access_tier(value: &str) -> Result<AccessTier, StorageError> {
    match value {
        "free" => Ok(AccessTier::Free),
        "free_tier" => Ok(AccessTier::FreeTier),
        "trial" => Ok(AccessTier::Trial),
        "paid" => Ok(AccessTier::Paid),
        "unknown" => Ok(AccessTier::Unknown),
        _ => Err(StorageError::Backend(format!(
            "unsupported model access tier '{value}' in SQLite"
        ))),
    }
}

fn optional_bool(value: Option<i64>) -> Option<bool> {
    value.map(|value| value != 0)
}

#[async_trait]
impl ModelCatalogOverrideStore for SqliteModelCatalogOverrideStore {
    async fn load_overrides(&self) -> Result<ModelCatalogOverrides, StorageError> {
        let connection = self.connection.lock().await;
        let mut overrides = ModelCatalogOverrides::default();

        {
            let mut statement = connection
                .prepare("SELECT provider, enabled, disabled_reason FROM provider_catalog_overrides ORDER BY provider")
                .map_err(backend)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .map_err(backend)?;
            for row in rows {
                let (provider, enabled, disabled_reason) = row.map_err(backend)?;
                overrides.providers.insert(
                    provider,
                    ProviderOverride {
                        enabled: optional_bool(enabled),
                        disabled_reason,
                    },
                );
            }
        }

        {
            let mut statement = connection
                .prepare("SELECT provider, model, enabled, access_tier, disabled_reason FROM model_catalog_overrides ORDER BY provider, model")
                .map_err(backend)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                })
                .map_err(backend)?;
            for row in rows {
                let (provider, model, enabled, access_tier, disabled_reason) =
                    row.map_err(backend)?;
                let access_tier = access_tier.as_deref().map(parse_access_tier).transpose()?;
                overrides.models.insert(
                    (provider, model),
                    ModelOverride {
                        enabled: optional_bool(enabled),
                        access_tier,
                        disabled_reason,
                    },
                );
            }
        }

        Ok(overrides)
    }

    async fn set_provider_override(
        &self,
        provider: &str,
        value: ProviderOverride,
    ) -> Result<(), StorageError> {
        let connection = self.connection.lock().await;
        if value.enabled.is_none() && value.disabled_reason.is_none() {
            connection
                .execute(
                    "DELETE FROM provider_catalog_overrides WHERE provider = ?1",
                    [provider],
                )
                .map_err(backend)?;
            return Ok(());
        }
        connection
            .execute(
                "INSERT INTO provider_catalog_overrides(provider, enabled, disabled_reason, updated_at) VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP) ON CONFLICT(provider) DO UPDATE SET enabled = excluded.enabled, disabled_reason = excluded.disabled_reason, updated_at = CURRENT_TIMESTAMP",
                params![provider, value.enabled.map(|enabled| if enabled { 1_i64 } else { 0_i64 }), value.disabled_reason],
            )
            .map_err(backend)?;
        Ok(())
    }

    async fn clear_provider_override(&self, provider: &str) -> Result<(), StorageError> {
        let connection = self.connection.lock().await;
        connection
            .execute(
                "DELETE FROM provider_catalog_overrides WHERE provider = ?1",
                [provider],
            )
            .map_err(backend)?;
        Ok(())
    }

    async fn set_model_override(
        &self,
        provider: &str,
        model: &str,
        value: ModelOverride,
    ) -> Result<(), StorageError> {
        let connection = self.connection.lock().await;
        if value.enabled.is_none() && value.access_tier.is_none() && value.disabled_reason.is_none()
        {
            connection
                .execute(
                    "DELETE FROM model_catalog_overrides WHERE provider = ?1 AND model = ?2",
                    params![provider, model],
                )
                .map_err(backend)?;
            return Ok(());
        }
        connection
            .execute(
                "INSERT INTO model_catalog_overrides(provider, model, enabled, access_tier, disabled_reason, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP) ON CONFLICT(provider, model) DO UPDATE SET enabled = excluded.enabled, access_tier = excluded.access_tier, disabled_reason = excluded.disabled_reason, updated_at = CURRENT_TIMESTAMP",
                params![
                    provider,
                    model,
                    value.enabled.map(|enabled| if enabled { 1_i64 } else { 0_i64 }),
                    value.access_tier.map(access_tier_name),
                    value.disabled_reason
                ],
            )
            .map_err(backend)?;
        Ok(())
    }

    async fn clear_model_override(&self, provider: &str, model: &str) -> Result<(), StorageError> {
        let connection = self.connection.lock().await;
        connection
            .execute(
                "DELETE FROM model_catalog_overrides WHERE provider = ?1 AND model = ?2",
                params![provider, model],
            )
            .map_err(backend)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::SqliteModelCatalogOverrideStore;
    use crate::ModelCatalogOverrideStore;
    use agent_models::{AccessTier, ModelCatalogOverrides, ModelOverride, ProviderOverride};

    struct TestDatabase(PathBuf);

    impl TestDatabase {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "model-catalog-overrides-test-{}-{}.sqlite",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            Self(path)
        }
    }

    impl Drop for TestDatabase {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
            let _ = std::fs::remove_file(self.0.with_extension("sqlite-shm"));
            let _ = std::fs::remove_file(self.0.with_extension("sqlite-wal"));
        }
    }

    #[tokio::test]
    async fn persists_sparse_provider_and_model_overrides_and_can_clear_them() {
        let database = TestDatabase::new();
        let store = SqliteModelCatalogOverrideStore::open(&database.0).unwrap();

        store
            .set_provider_override(
                "groq",
                ProviderOverride {
                    enabled: Some(false),
                    disabled_reason: Some("temporary quota issue".into()),
                },
            )
            .await
            .unwrap();
        store
            .set_model_override(
                "gemini",
                "gemini-3.5-flash-lite",
                ModelOverride {
                    enabled: Some(true),
                    access_tier: Some(AccessTier::FreeTier),
                    disabled_reason: None,
                },
            )
            .await
            .unwrap();

        let expected = ModelCatalogOverrides {
            providers: [(
                "groq".into(),
                ProviderOverride {
                    enabled: Some(false),
                    disabled_reason: Some("temporary quota issue".into()),
                },
            )]
            .into(),
            models: [(
                ("gemini".into(), "gemini-3.5-flash-lite".into()),
                ModelOverride {
                    enabled: Some(true),
                    access_tier: Some(AccessTier::FreeTier),
                    disabled_reason: None,
                },
            )]
            .into(),
        };
        assert_eq!(store.load_overrides().await.unwrap(), expected);

        store.clear_provider_override("groq").await.unwrap();
        store
            .clear_model_override("gemini", "gemini-3.5-flash-lite")
            .await
            .unwrap();
        assert_eq!(
            store.load_overrides().await.unwrap(),
            ModelCatalogOverrides::default()
        );
    }
}
