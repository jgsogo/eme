//! SQLite persistence for media profiles, preferences, catalogue and viewing activity.
use std::{path::Path, sync::Arc};

use agent_storage::{ExternalConnectionId, FamilyId, UserId};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::{
    ContentRestrictions, FamilyMediaPreferences, MediaItem, MediaProfile, MediaProfileId,
    MediaRating, MediaWatchEvent, MediaWatchlistItem, ProfilePreferences, ProviderSyncState, StreamingOffer,
};

/// SQLite-backed persistence for media data.
///
/// Family membership remains owned by `agent-storage`; this store only scopes
/// media records by the authoritative family ID supplied by the application.
#[derive(Clone)]
pub struct MediaStore {
    connection: Arc<Mutex<Connection>>,
}

impl MediaStore {
    /// Open or create a media database at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MediaStoreError> {
        let connection = Connection::open(path).map_err(storage)?;
        Self::from_connection(connection)
    }

    /// Create an isolated in-memory media database (primarily useful in tests).
    pub fn open_in_memory() -> Result<Self, MediaStoreError> {
        Self::from_connection(Connection::open_in_memory().map_err(storage)?)
    }

    fn from_connection(connection: Connection) -> Result<Self, MediaStoreError> {
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(storage)?;
        connection
            .execute_batch(include_str!("../migrations/0001_media.sql"))
            .map_err(storage)?;
        connection
            .execute_batch(include_str!(
                "../migrations/0002_media_external_connections.sql"
            ))
            .map_err(storage)?;
        connection
            .execute_batch(include_str!("../migrations/0003_media_watchlist.sql"))
            .map_err(storage)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Insert or update a family-managed media profile without changing its family.
    pub async fn save_profile(&self, profile: &MediaProfile) -> Result<(), MediaStoreError> {
        if profile.name.trim().is_empty() {
            return Err(MediaStoreError::Invalid(
                "profile name must not be empty".into(),
            ));
        }
        let payload = encode(profile)?;
        let now = Utc::now().to_rfc3339();
        let connection = self.connection.lock().await;
        let changed = connection
            .execute(
                "INSERT INTO media_profiles(id,family_id,user_id,name,active,payload,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?7)
                 ON CONFLICT(id) DO UPDATE SET
                    user_id=excluded.user_id,
                    name=excluded.name,
                    active=excluded.active,
                    payload=excluded.payload,
                    updated_at=excluded.updated_at
                 WHERE media_profiles.family_id=excluded.family_id",
                params![
                    profile.id.to_string(),
                    profile.family_id.to_string(),
                    profile.user_id.map(|id| id.to_string()),
                    profile.name.trim(),
                    profile.active,
                    payload,
                    now,
                ],
            )
            .map_err(storage)?;
        if changed == 0 {
            return Err(MediaStoreError::ProfileFamilyMismatch);
        }
        Ok(())
    }

    /// List profiles belonging to one family; no membership data is duplicated here.
    pub async fn list_profiles(
        &self,
        family_id: FamilyId,
    ) -> Result<Vec<MediaProfile>, MediaStoreError> {
        let connection = self.connection.lock().await;
        let mut statement = connection
            .prepare("SELECT payload FROM media_profiles WHERE family_id=?1 ORDER BY name,id")
            .map_err(storage)?;
        let rows = statement
            .query_map([family_id.to_string()], |row| row.get::<_, String>(0))
            .map_err(storage)?;
        rows.map(|row| decode(&row.map_err(storage)?)).collect()
    }

    /// Load a profile only within the requested family boundary.
    pub async fn get_profile(
        &self,
        family_id: FamilyId,
        profile_id: MediaProfileId,
    ) -> Result<Option<MediaProfile>, MediaStoreError> {
        let connection = self.connection.lock().await;
        let payload = connection
            .query_row(
                "SELECT payload FROM media_profiles WHERE family_id=?1 AND id=?2",
                params![family_id.to_string(), profile_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(storage)?;
        payload.as_deref().map(decode).transpose()
    }

    /// Remove a profile and its dependent preferences and activity.
    pub async fn delete_profile(
        &self,
        family_id: FamilyId,
        profile_id: MediaProfileId,
    ) -> Result<(), MediaStoreError> {
        let connection = self.connection.lock().await;
        let changed = connection
            .execute(
                "DELETE FROM media_profiles WHERE family_id=?1 AND id=?2",
                params![family_id.to_string(), profile_id.to_string()],
            )
            .map_err(storage)?;
        if changed == 0 {
            return Err(MediaStoreError::NotFound);
        }
        Ok(())
    }

    pub async fn save_profile_preferences(
        &self,
        preferences: &ProfilePreferences,
    ) -> Result<(), MediaStoreError> {
        let payload = encode(preferences)?;
        let connection = self.connection.lock().await;
        connection
            .execute(
                "INSERT INTO profile_preferences(profile_id,payload,updated_at) VALUES(?1,?2,?3)
                 ON CONFLICT(profile_id) DO UPDATE SET payload=excluded.payload,updated_at=excluded.updated_at",
                params![preferences.profile_id.to_string(), payload, Utc::now().to_rfc3339()],
            )
            .map_err(storage)?;
        Ok(())
    }

    pub async fn get_profile_preferences(
        &self,
        profile_id: MediaProfileId,
    ) -> Result<Option<ProfilePreferences>, MediaStoreError> {
        self.get_payload(
            "SELECT payload FROM profile_preferences WHERE profile_id=?1",
            profile_id.to_string(),
        )
        .await
    }

    pub async fn save_content_restrictions(
        &self,
        restrictions: &ContentRestrictions,
    ) -> Result<(), MediaStoreError> {
        let payload = encode(restrictions)?;
        let connection = self.connection.lock().await;
        connection
            .execute(
                "INSERT INTO profile_restrictions(profile_id,payload,updated_at) VALUES(?1,?2,?3)
                 ON CONFLICT(profile_id) DO UPDATE SET payload=excluded.payload,updated_at=excluded.updated_at",
                params![restrictions.profile_id.to_string(), payload, Utc::now().to_rfc3339()],
            )
            .map_err(storage)?;
        Ok(())
    }

    pub async fn get_content_restrictions(
        &self,
        profile_id: MediaProfileId,
    ) -> Result<Option<ContentRestrictions>, MediaStoreError> {
        self.get_payload(
            "SELECT payload FROM profile_restrictions WHERE profile_id=?1",
            profile_id.to_string(),
        )
        .await
    }

    pub async fn save_family_preferences(
        &self,
        preferences: &FamilyMediaPreferences,
    ) -> Result<(), MediaStoreError> {
        let payload = encode(preferences)?;
        let connection = self.connection.lock().await;
        connection
            .execute(
                "INSERT INTO family_preferences(family_id,payload,updated_at) VALUES(?1,?2,?3)
                 ON CONFLICT(family_id) DO UPDATE SET payload=excluded.payload,updated_at=excluded.updated_at",
                params![preferences.family_id.to_string(), payload, Utc::now().to_rfc3339()],
            )
            .map_err(storage)?;
        Ok(())
    }

    pub async fn get_family_preferences(
        &self,
        family_id: FamilyId,
    ) -> Result<Option<FamilyMediaPreferences>, MediaStoreError> {
        self.get_payload(
            "SELECT payload FROM family_preferences WHERE family_id=?1",
            family_id.to_string(),
        )
        .await
    }

    pub async fn save_media_item(&self, item: &MediaItem) -> Result<(), MediaStoreError> {
        let payload = encode(item)?;
        let connection = self.connection.lock().await;
        connection
            .execute(
                "INSERT INTO media_items(id,kind,payload,updated_at) VALUES(?1,?2,?3,?4)
                 ON CONFLICT(id) DO UPDATE SET kind=excluded.kind,payload=excluded.payload,updated_at=excluded.updated_at",
                params![item.id.to_string(), format!("{:?}", item.kind).to_lowercase(), payload, Utc::now().to_rfc3339()],
            )
            .map_err(storage)?;
        Ok(())
    }

    pub async fn get_media_item(
        &self,
        id: crate::MediaItemId,
    ) -> Result<Option<MediaItem>, MediaStoreError> {
        self.get_payload(
            "SELECT payload FROM media_items WHERE id=?1",
            id.to_string(),
        )
        .await
    }

    /// Record a viewing event idempotently (same profile, item, timestamp and source).
    pub async fn record_watch_event(&self, event: &MediaWatchEvent) -> Result<(), MediaStoreError> {
        let payload = encode(event)?;
        let connection = self.connection.lock().await;
        connection
            .execute(
                "INSERT OR IGNORE INTO watch_history(profile_id,media_item_id,watched_at,source,payload)
                 VALUES(?1,?2,?3,?4,?5)",
                params![
                    event.profile_id.to_string(),
                    event.media_item_id.to_string(),
                    event.watched_at.to_rfc3339(),
                    format!("{:?}", event.source).to_lowercase(),
                    payload,
                ],
            )
            .map_err(storage)?;
        Ok(())
    }

    pub async fn list_watch_history(
        &self,
        profile_id: MediaProfileId,
    ) -> Result<Vec<MediaWatchEvent>, MediaStoreError> {
        self.list_payloads(
            "SELECT payload FROM watch_history WHERE profile_id=?1 ORDER BY watched_at DESC,id DESC",
            profile_id.to_string(),
        )
        .await
    }

    /// Store a profile rating, replacing the previous rating from the same source.
    pub async fn save_rating(&self, rating: &MediaRating) -> Result<(), MediaStoreError> {
        if !rating.value.is_finite() || !(0.0..=5.0).contains(&rating.value) {
            return Err(MediaStoreError::Invalid(
                "rating must be between 0 and 5".into(),
            ));
        }
        let payload = encode(rating)?;
        let connection = self.connection.lock().await;
        connection
            .execute(
                "INSERT INTO media_ratings(profile_id,media_item_id,source,value,rated_at,payload)
                 VALUES(?1,?2,?3,?4,?5,?6)
                 ON CONFLICT(profile_id,media_item_id,source) DO UPDATE SET
                    value=excluded.value,rated_at=excluded.rated_at,payload=excluded.payload",
                params![
                    rating.profile_id.to_string(),
                    rating.media_item_id.to_string(),
                    format!("{:?}", rating.source).to_lowercase(),
                    rating.value,
                    rating.rated_at.to_rfc3339(),
                    payload,
                ],
            )
            .map_err(storage)?;
        Ok(())
    }

    pub async fn list_ratings(
        &self,
        profile_id: MediaProfileId,
    ) -> Result<Vec<MediaRating>, MediaStoreError> {
        self.list_payloads(
            "SELECT payload FROM media_ratings WHERE profile_id=?1 ORDER BY rated_at DESC",
            profile_id.to_string(),
        )
        .await
    }

    pub async fn save_streaming_offer(
        &self,
        offer: &StreamingOffer,
    ) -> Result<(), MediaStoreError> {
        let payload = encode(offer)?;
        let connection = self.connection.lock().await;
        connection
            .execute(
                "INSERT INTO streaming_offers(media_item_id,service_id,offer_type,region,payload,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6)
                 ON CONFLICT(media_item_id,service_id,offer_type,region) DO UPDATE SET
                    payload=excluded.payload,updated_at=excluded.updated_at",
                params![
                    offer.media_item_id.to_string(),
                    offer.service_id,
                    format!("{:?}", offer.offer_type).to_lowercase(),
                    offer.region,
                    payload,
                    Utc::now().to_rfc3339(),
                ],
            )
            .map_err(storage)?;
        Ok(())
    }

    pub async fn list_streaming_offers(
        &self,
        item_id: crate::MediaItemId,
        region: &str,
    ) -> Result<Vec<StreamingOffer>, MediaStoreError> {
        let connection = self.connection.lock().await;
        let mut statement = connection
            .prepare("SELECT payload FROM streaming_offers WHERE media_item_id=?1 AND region=?2 ORDER BY service_id,offer_type")
            .map_err(storage)?;
        let rows = statement
            .query_map(params![item_id.to_string(), region], |row| {
                row.get::<_, String>(0)
            })
            .map_err(storage)?;
        rows.map(|row| decode(&row.map_err(storage)?)).collect()
    }

    /// Persist a short-lived, hashed OAuth state bound to the initiating user and profile.
    pub async fn create_pending_trakt_link(
        &self,
        state: &str,
        pending: &PendingTraktLink,
        expires_at: DateTime<Utc>,
    ) -> Result<(), MediaStoreError> {
        if state.trim().is_empty() {
            return Err(MediaStoreError::Invalid(
                "OAuth state must not be empty".into(),
            ));
        }
        let state_hash = hash_state(state);
        let payload = encode(pending)?;
        let now = Utc::now().to_rfc3339();
        let connection = self.connection.lock().await;
        connection
            .execute(
                "INSERT INTO pending_media_connections(state_hash,provider,payload,expires_at,created_at)
                 VALUES(?1,'trakt',?2,?3,?4)",
                params![state_hash, payload, expires_at.to_rfc3339(), now],
            )
            .map_err(storage)?;
        Ok(())
    }

    /// Consume a pending Trakt state exactly once and reject expired attempts.
    pub async fn consume_pending_trakt_link(
        &self,
        state: &str,
        now: DateTime<Utc>,
    ) -> Result<PendingTraktLink, MediaStoreError> {
        let state_hash = hash_state(state);
        let mut connection = self.connection.lock().await;
        let transaction = connection.transaction().map_err(storage)?;
        let row = transaction
            .query_row(
                "SELECT payload,expires_at FROM pending_media_connections
                 WHERE state_hash=?1 AND provider='trakt'",
                [state_hash],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(storage)?;
        let Some((payload, expires_at)) = row else {
            return Err(MediaStoreError::PendingNotFound);
        };
        let expires_at = DateTime::parse_from_rfc3339(&expires_at)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|error| MediaStoreError::Storage(error.to_string()))?;
        transaction
            .execute(
                "DELETE FROM pending_media_connections WHERE state_hash=?1",
                [state_hash],
            )
            .map_err(storage)?;
        transaction.commit().map_err(storage)?;
        if now > expires_at {
            return Err(MediaStoreError::PendingExpired);
        }
        decode(&payload)
    }

    /// Associate a profile with an existing encrypted EME external connection.
    pub async fn save_external_connection_link(
        &self,
        link: &MediaExternalConnectionLink,
    ) -> Result<(), MediaStoreError> {
        let payload = encode(link)?;
        let connection = self.connection.lock().await;
        let profile_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM media_profiles WHERE id=?1 AND family_id=?2)",
                params![link.profile_id.to_string(), link.family_id.to_string()],
                |row| row.get(0),
            )
            .map_err(storage)?;
        if !profile_exists {
            return Err(MediaStoreError::NotFound);
        }
        connection
            .execute(
                "INSERT INTO media_external_connections(
                    profile_id,family_id,provider,external_connection_id,owner_id,
                    provider_subject,display_name,linked_at,payload
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
                 ON CONFLICT(profile_id,provider) DO UPDATE SET
                    family_id=excluded.family_id,
                    external_connection_id=excluded.external_connection_id,
                    owner_id=excluded.owner_id,
                    provider_subject=excluded.provider_subject,
                    display_name=excluded.display_name,
                    linked_at=excluded.linked_at,
                    payload=excluded.payload",
                params![
                    link.profile_id.to_string(),
                    link.family_id.to_string(),
                    link.provider,
                    link.external_connection_id.to_string(),
                    link.owner_id.to_string(),
                    link.provider_subject,
                    link.display_name,
                    link.linked_at.to_rfc3339(),
                    payload,
                ],
            )
            .map_err(storage)?;
        Ok(())
    }

    pub async fn get_external_connection_link(
        &self,
        profile_id: MediaProfileId,
        provider: &str,
    ) -> Result<Option<MediaExternalConnectionLink>, MediaStoreError> {
        let connection = self.connection.lock().await;
        let payload = connection
            .query_row(
                "SELECT payload FROM media_external_connections WHERE profile_id=?1 AND provider=?2",
                params![profile_id.to_string(), provider],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(storage)?;
        payload.as_deref().map(decode).transpose()
    }

    /// Remove a profile's provider link; token revocation remains the caller's responsibility.
    pub async fn delete_external_connection_link(
        &self,
        profile_id: MediaProfileId,
        provider: &str,
    ) -> Result<(), MediaStoreError> {
        let connection = self.connection.lock().await;
        connection
            .execute(
                "DELETE FROM media_external_connections WHERE profile_id=?1 AND provider=?2",
                params![profile_id.to_string(), provider],
            )
            .map_err(storage)?;
        Ok(())
    }


    pub async fn save_watchlist_item(
        &self,
        item: &MediaWatchlistItem,
    ) -> Result<(), MediaStoreError> {
        let payload = encode(item)?;
        let connection = self.connection.lock().await;
        connection
            .execute(
                "INSERT INTO media_watchlist(profile_id,media_item_id,source,added_at,payload)
                 VALUES(?1,?2,?3,?4,?5)
                 ON CONFLICT(profile_id,media_item_id,source) DO UPDATE SET
                    added_at=excluded.added_at,payload=excluded.payload",
                params![
                    item.profile_id.to_string(),
                    item.media_item_id.to_string(),
                    format!("{:?}", item.source).to_lowercase(),
                    item.added_at.to_rfc3339(),
                    payload,
                ],
            )
            .map_err(storage)?;
        Ok(())
    }

    pub async fn list_watchlist(
        &self,
        profile_id: MediaProfileId,
    ) -> Result<Vec<MediaWatchlistItem>, MediaStoreError> {
        self.list_payloads(
            "SELECT payload FROM media_watchlist WHERE profile_id=?1 ORDER BY added_at DESC",
            profile_id.to_string(),
        )
        .await
    }

    /// Replace a provider's watchlist atomically so removals are reflected locally.
    pub async fn replace_watchlist(
        &self,
        profile_id: MediaProfileId,
        source: crate::WatchHistorySource,
        items: &[MediaWatchlistItem],
    ) -> Result<(), MediaStoreError> {
        if items.iter().any(|item| item.profile_id != profile_id || item.source != source) {
            return Err(MediaStoreError::Invalid(
                "watchlist items must match the target profile and source".into(),
            ));
        }
        let mut connection = self.connection.lock().await;
        let transaction = connection.transaction().map_err(storage)?;
        transaction
            .execute(
                "DELETE FROM media_watchlist WHERE profile_id=?1 AND source=?2",
                params![profile_id.to_string(), format!("{:?}", source).to_lowercase()],
            )
            .map_err(storage)?;
        for item in items {
            transaction
                .execute(
                    "INSERT INTO media_watchlist(profile_id,media_item_id,source,added_at,payload)
                     VALUES(?1,?2,?3,?4,?5)",
                    params![
                        item.profile_id.to_string(),
                        item.media_item_id.to_string(),
                        format!("{:?}", item.source).to_lowercase(),
                        item.added_at.to_rfc3339(),
                        encode(item)?,
                    ],
                )
                .map_err(storage)?;
        }
        transaction.commit().map_err(storage)?;
        Ok(())
    }

    /// Replace all ratings from one provider after a full provider snapshot.
    pub async fn replace_ratings(
        &self,
        profile_id: MediaProfileId,
        source: crate::WatchHistorySource,
        ratings: &[MediaRating],
    ) -> Result<(), MediaStoreError> {
        if ratings.iter().any(|rating| {
            rating.profile_id != profile_id
                || rating.source != source
                || !rating.value.is_finite()
                || !(0.0..=5.0).contains(&rating.value)
        }) {
            return Err(MediaStoreError::Invalid(
                "ratings must match the target profile/source and use the 0–5 scale".into(),
            ));
        }
        let mut connection = self.connection.lock().await;
        let transaction = connection.transaction().map_err(storage)?;
        transaction
            .execute(
                "DELETE FROM media_ratings WHERE profile_id=?1 AND source=?2",
                params![profile_id.to_string(), format!("{:?}", source).to_lowercase()],
            )
            .map_err(storage)?;
        for rating in ratings {
            transaction
                .execute(
                    "INSERT INTO media_ratings(profile_id,media_item_id,source,value,rated_at,payload)
                     VALUES(?1,?2,?3,?4,?5,?6)",
                    params![
                        rating.profile_id.to_string(),
                        rating.media_item_id.to_string(),
                        format!("{:?}", rating.source).to_lowercase(),
                        rating.value,
                        rating.rated_at.to_rfc3339(),
                        encode(rating)?,
                    ],
                )
                .map_err(storage)?;
        }
        transaction.commit().map_err(storage)?;
        Ok(())
    }

    pub async fn get_provider_sync_state(
        &self,
        profile_id: MediaProfileId,
        provider: &str,
    ) -> Result<Option<ProviderSyncState>, MediaStoreError> {
        let connection = self.connection.lock().await;
        let row = connection
            .query_row(
                "SELECT cursor,last_synced_at,status FROM provider_sync_state
                 WHERE profile_id=?1 AND provider=?2",
                params![profile_id.to_string(), provider],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(storage)?;
        row.map(|(cursor, last_synced_at, status)| {
            let last_synced_at = last_synced_at
                .map(|value| {
                    DateTime::parse_from_rfc3339(&value)
                        .map(|date| date.with_timezone(&Utc))
                        .map_err(|error| MediaStoreError::Storage(error.to_string()))
                })
                .transpose()?;
            Ok(ProviderSyncState {
                profile_id,
                provider: provider.to_owned(),
                cursor,
                last_synced_at,
                status,
            })
        })
        .transpose()
    }

    pub async fn save_provider_sync_state(
        &self,
        state: &ProviderSyncState,
    ) -> Result<(), MediaStoreError> {
        let connection = self.connection.lock().await;
        connection
            .execute(
                "INSERT INTO provider_sync_state(profile_id,provider,cursor,last_synced_at,status)
                 VALUES(?1,?2,?3,?4,?5)
                 ON CONFLICT(profile_id,provider) DO UPDATE SET
                    cursor=excluded.cursor,last_synced_at=excluded.last_synced_at,status=excluded.status",
                params![
                    state.profile_id.to_string(),
                    state.provider,
                    state.cursor,
                    state.last_synced_at.as_ref().map(|value| value.to_rfc3339()),
                    state.status,
                ],
            )
            .map_err(storage)?;
        Ok(())
    }

    async fn get_payload<T: serde::de::DeserializeOwned>(
        &self,
        sql: &str,
        key: String,
    ) -> Result<Option<T>, MediaStoreError> {
        let connection = self.connection.lock().await;
        let payload = connection
            .query_row(sql, [key], |row| row.get::<_, String>(0))
            .optional()
            .map_err(storage)?;
        payload.as_deref().map(decode).transpose()
    }

    async fn list_payloads<T: serde::de::DeserializeOwned>(
        &self,
        sql: &str,
        key: String,
    ) -> Result<Vec<T>, MediaStoreError> {
        let connection = self.connection.lock().await;
        let mut statement = connection.prepare(sql).map_err(storage)?;
        let rows = statement
            .query_map([key], |row| row.get::<_, String>(0))
            .map_err(storage)?;
        rows.map(|row| decode(&row.map_err(storage)?)).collect()
    }
}

/// Pending profile association carried through a provider OAuth callback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingTraktLink {
    pub owner_id: UserId,
    pub family_id: FamilyId,
    pub profile_id: MediaProfileId,
}

/// Link from a media profile to a credential stored by agent-storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaExternalConnectionLink {
    pub owner_id: UserId,
    pub family_id: FamilyId,
    pub profile_id: MediaProfileId,
    pub provider: String,
    pub external_connection_id: ExternalConnectionId,
    pub provider_subject: String,
    pub display_name: Option<String>,
    pub linked_at: DateTime<Utc>,
}

fn hash_state(state: &str) -> String {
    Sha256::digest(state.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn encode<T: serde::Serialize>(value: &T) -> Result<String, MediaStoreError> {
    serde_json::to_string(value).map_err(|error| MediaStoreError::Serialization(error.to_string()))
}

fn decode<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, MediaStoreError> {
    serde_json::from_str(value).map_err(|error| MediaStoreError::Serialization(error.to_string()))
}

fn storage(error: rusqlite::Error) -> MediaStoreError {
    MediaStoreError::Storage(error.to_string())
}

/// Failures returned by the media SQLite store.
#[derive(Debug, thiserror::Error)]
pub enum MediaStoreError {
    #[error("media record was not found")]
    NotFound,
    #[error("media profile cannot be moved to a different family")]
    ProfileFamilyMismatch,
    #[error("invalid media data: {0}")]
    Invalid(String),
    #[error("media storage failed: {0}")]
    Storage(String),
    #[error("media serialization failed: {0}")]
    Serialization(String),
    #[error("pending provider authorization was not found or was already consumed")]
    PendingNotFound,
    #[error("pending provider authorization has expired")]
    PendingExpired,
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::{ExternalId, MediaItemId, MediaKind, OfferType, WatchHistorySource};

    fn item() -> MediaItem {
        MediaItem {
            id: MediaItemId::new(),
            kind: MediaKind::Movie,
            title: "Example".into(),
            original_title: Some("Example Original".into()),
            overview: None,
            release_date: None,
            original_language: Some("en".into()),
            genres: vec!["Drama".into()],
            runtime_minutes: Some(100),
            external_ids: vec![ExternalId {
                provider: "tmdb".into(),
                value: "42".into(),
            }],
        }
    }

    #[tokio::test]
    async fn profiles_are_scoped_to_their_family_and_can_be_managed_without_users() {
        let store = MediaStore::open_in_memory().unwrap();
        let family = FamilyId::new();
        let other_family = FamilyId::new();
        let profile = MediaProfile::new(family, "Child");
        store.save_profile(&profile).await.unwrap();

        assert!(profile.user_id.is_none());
        assert_eq!(
            store.list_profiles(family).await.unwrap(),
            vec![profile.clone()]
        );
        assert!(store.list_profiles(other_family).await.unwrap().is_empty());
        assert!(
            store
                .get_profile(other_family, profile.id)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn profile_preferences_and_restrictions_round_trip() {
        let store = MediaStore::open_in_memory().unwrap();
        let profile = MediaProfile::new(FamilyId::new(), "Adult");
        store.save_profile(&profile).await.unwrap();

        let mut preferences = ProfilePreferences::new(profile.id);
        preferences.liked_genres.push("Science Fiction".into());
        store.save_profile_preferences(&preferences).await.unwrap();
        assert_eq!(
            store.get_profile_preferences(profile.id).await.unwrap(),
            Some(preferences)
        );

        let mut restrictions = ContentRestrictions::new(profile.id);
        restrictions.maximum_certification = Some("12".into());
        store
            .save_content_restrictions(&restrictions)
            .await
            .unwrap();
        assert_eq!(
            store.get_content_restrictions(profile.id).await.unwrap(),
            Some(restrictions)
        );
    }

    #[tokio::test]
    async fn family_preferences_are_stored_independently_of_profiles() {
        let store = MediaStore::open_in_memory().unwrap();
        let family_id = FamilyId::new();
        let mut preferences = FamilyMediaPreferences::new(family_id);
        preferences.liked_genres.push("Comedy".into());
        store.save_family_preferences(&preferences).await.unwrap();
        assert_eq!(
            store.get_family_preferences(family_id).await.unwrap(),
            Some(preferences)
        );
    }

    #[tokio::test]
    async fn watch_history_is_idempotent_and_ratings_are_source_specific() {
        let store = MediaStore::open_in_memory().unwrap();
        let profile = MediaProfile::new(FamilyId::new(), "Viewer");
        store.save_profile(&profile).await.unwrap();
        let item = item();
        store.save_media_item(&item).await.unwrap();
        let event = MediaWatchEvent {
            profile_id: profile.id,
            media_item_id: item.id,
            watched_at: Utc::now(),
            source: WatchHistorySource::Trakt,
        };
        store.record_watch_event(&event).await.unwrap();
        store.record_watch_event(&event).await.unwrap();
        assert_eq!(store.list_watch_history(profile.id).await.unwrap().len(), 1);

        let rating = MediaRating {
            profile_id: profile.id,
            media_item_id: item.id,
            value: 4.5,
            rated_at: Utc::now(),
            source: WatchHistorySource::Trakt,
        };
        store.save_rating(&rating).await.unwrap();
        assert_eq!(store.list_ratings(profile.id).await.unwrap(), vec![rating]);
    }

    #[tokio::test]
    async fn streaming_offers_round_trip_by_region() {
        let store = MediaStore::open_in_memory().unwrap();
        let item = item();
        store.save_media_item(&item).await.unwrap();
        let offer = StreamingOffer {
            media_item_id: item.id,
            service_id: "netflix_es".into(),
            service_name: "Netflix".into(),
            offer_type: OfferType::Subscription,
            region: "ES".into(),
            url: Some("https://example.invalid/watch".into()),
        };
        store.save_streaming_offer(&offer).await.unwrap();
        assert_eq!(
            store.list_streaming_offers(item.id, "ES").await.unwrap(),
            vec![offer]
        );
        assert!(
            store
                .list_streaming_offers(item.id, "US")
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn watchlist_replacement_removes_items_no_longer_present_at_provider() {
        let store = MediaStore::open_in_memory().unwrap();
        let profile = MediaProfile::new(FamilyId::new(), "Viewer");
        store.save_profile(&profile).await.unwrap();
        let first = item();
        let second = item();
        store.save_media_item(&first).await.unwrap();
        store.save_media_item(&second).await.unwrap();
        let first_entry = MediaWatchlistItem {
            profile_id: profile.id,
            media_item_id: first.id,
            added_at: Utc::now(),
            source: WatchHistorySource::Trakt,
        };
        let second_entry = MediaWatchlistItem {
            profile_id: profile.id,
            media_item_id: second.id,
            added_at: Utc::now(),
            source: WatchHistorySource::Trakt,
        };

        store
            .replace_watchlist(
                profile.id,
                WatchHistorySource::Trakt,
                &[first_entry, second_entry.clone()],
            )
            .await
            .unwrap();
        store
            .replace_watchlist(profile.id, WatchHistorySource::Trakt, &[second_entry.clone()])
            .await
            .unwrap();

        assert_eq!(store.list_watchlist(profile.id).await.unwrap(), vec![second_entry]);
    }

    #[tokio::test]
    async fn provider_sync_cursor_round_trips() {
        let store = MediaStore::open_in_memory().unwrap();
        let profile = MediaProfile::new(FamilyId::new(), "Viewer");
        store.save_profile(&profile).await.unwrap();
        let state = ProviderSyncState {
            profile_id: profile.id,
            provider: "trakt".into(),
            cursor: Some("{\"movies_watched_at\":\"2026-01-01T00:00:00Z\"}".into()),
            last_synced_at: Some(Utc::now()),
            status: "complete".into(),
        };
        store.save_provider_sync_state(&state).await.unwrap();
        assert_eq!(
            store
                .get_provider_sync_state(profile.id, "trakt")
                .await
                .unwrap(),
            Some(state)
        );
    }

    #[tokio::test]
    async fn pending_trakt_authorization_is_hashed_and_consumed_once() {
        let store = MediaStore::open_in_memory().unwrap();
        let owner_id = UserId::new();
        let family_id = FamilyId::new();
        let profile = MediaProfile::new(family_id, "Child");
        store.save_profile(&profile).await.unwrap();
        let pending = PendingTraktLink {
            owner_id,
            family_id,
            profile_id: profile.id,
        };
        let now = Utc::now();
        store
            .create_pending_trakt_link(
                "one-time-state",
                &pending,
                now + chrono::Duration::minutes(5),
            )
            .await
            .unwrap();

        assert_eq!(
            store
                .consume_pending_trakt_link("one-time-state", now)
                .await
                .unwrap(),
            pending
        );
        assert!(matches!(
            store
                .consume_pending_trakt_link("one-time-state", now)
                .await,
            Err(MediaStoreError::PendingNotFound)
        ));
    }

    #[tokio::test]
    async fn expired_trakt_authorization_state_is_rejected_and_removed() {
        let store = MediaStore::open_in_memory().unwrap();
        let family_id = FamilyId::new();
        let profile = MediaProfile::new(family_id, "Child");
        store.save_profile(&profile).await.unwrap();
        let now = Utc::now();
        store
            .create_pending_trakt_link(
                "expired-state",
                &PendingTraktLink {
                    owner_id: UserId::new(),
                    family_id,
                    profile_id: profile.id,
                },
                now - chrono::Duration::seconds(1),
            )
            .await
            .unwrap();

        assert!(matches!(
            store.consume_pending_trakt_link("expired-state", now).await,
            Err(MediaStoreError::PendingExpired)
        ));
        assert!(matches!(
            store.consume_pending_trakt_link("expired-state", now).await,
            Err(MediaStoreError::PendingNotFound)
        ));
    }

    #[tokio::test]
    async fn rating_values_outside_normalized_range_are_rejected() {
        let store = MediaStore::open_in_memory().unwrap();
        let profile = MediaProfile::new(FamilyId::new(), "Viewer");
        store.save_profile(&profile).await.unwrap();
        let item = item();
        store.save_media_item(&item).await.unwrap();
        let rating = MediaRating {
            profile_id: profile.id,
            media_item_id: item.id,
            value: 6.0,
            rated_at: Utc::now(),
            source: WatchHistorySource::Eme,
        };
        assert!(matches!(
            store.save_rating(&rating).await,
            Err(MediaStoreError::Invalid(_))
        ));
    }
}
