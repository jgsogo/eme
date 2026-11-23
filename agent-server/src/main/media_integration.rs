//! Compose optional media providers and their persistent stores from server configuration.
use std::{fs, path::Path, sync::Arc, time::Duration};

use agent_server::MediaApiState;
use agent_storage::{ExternalConnectionStore, FamilyStore, SqliteExternalConnectionStore};
use anyhow::{Context, Result, bail};
use media_tools::{
    MediaConfig, MediaStore, MediaToolServices, TmdbClient, TraktOAuthConfig, TraktOAuthService,
    TraktSyncService, WatchmodeClient,
};

pub struct MediaIntegration {
    pub tools: MediaToolServices,
    pub api: MediaApiState,
}

impl MediaIntegration {
    pub fn from_env(data_dir: &Path, families: Arc<dyn FamilyStore>) -> Result<Self> {
        let media_dir = data_dir.join("media");
        fs::create_dir_all(&media_dir)
            .with_context(|| format!("creating media data directory {}", media_dir.display()))?;
        let config_path = media_dir.join("config.toml");
        let config = match fs::read_to_string(&config_path) {
            Ok(contents) => toml::from_str::<MediaConfig>(&contents)
                .with_context(|| format!("parsing {}", config_path.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => MediaConfig::default(),
            Err(error) => {
                return Err(error).with_context(|| format!("reading {}", config_path.display()));
            }
        };
        config
            .validate()
            .context("validating media configuration")?;

        let media = MediaStore::open(media_dir.join("media.sqlite"))
            .context("opening media catalogue and profile storage")?;

        let tmdb = std::env::var("TMDB_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|key| TmdbClient::new(key, config.language.clone(), config.country.clone()))
            .transpose()
            .context("configuring TMDB")?;
        let watchmode = std::env::var("WATCHMODE_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|key| {
                WatchmodeClient::with_cache_ttl(
                    key,
                    Duration::from_secs(config.availability_cache_ttl_seconds),
                )
            })
            .transpose()
            .context("configuring Watchmode")?;

        let trakt_client_id = std::env::var("TRAKT_CLIENT_ID").ok();
        let trakt_redirect_uri = std::env::var("TRAKT_REDIRECT_URI").ok();
        let encryption_key = std::env::var("EME_CREDENTIAL_ENCRYPTION_KEY").ok();
        let (trakt_oauth, trakt_sync) = match (trakt_client_id, trakt_redirect_uri) {
            (None, None) => (None, None),
            (Some(client_id), Some(redirect_uri)) => {
                let key = parse_encryption_key(encryption_key.as_deref().unwrap_or_default())
                    .context("Trakt OAuth requires EME_CREDENTIAL_ENCRYPTION_KEY")?;
                let external_store = Arc::new(
                    SqliteExternalConnectionStore::open(
                        data_dir.join("external_connections.sqlite"),
                        key,
                    )
                    .context("opening encrypted external connection storage for Trakt")?,
                );
                let external_connections: Arc<dyn ExternalConnectionStore> = external_store.clone();
                let oauth_config = TraktOAuthConfig::new(client_id.clone(), redirect_uri)
                    .context("validating Trakt OAuth configuration")?;
                let oauth = Arc::new(
                    TraktOAuthService::new(
                        oauth_config,
                        media.clone(),
                        families.clone(),
                        external_connections,
                    )
                    .map_err(anyhow::Error::msg)
                    .context("initializing Trakt OAuth")?,
                );
                let sync =
                    TraktSyncService::new(media.clone(), oauth.token_manager().clone(), client_id)
                        .map_err(anyhow::Error::msg)
                        .context("initializing Trakt synchronization")?;
                (Some(oauth), Some(sync))
            }
            _ => bail!("TRAKT_CLIENT_ID and TRAKT_REDIRECT_URI must be configured together"),
        };

        let tools =
            MediaToolServices::new(media.clone(), config, tmdb, watchmode, trakt_sync.clone());
        let api = MediaApiState {
            media: Some(media),
            families: Some(families),
            trakt_oauth,
            trakt_sync,
        };
        Ok(Self { tools, api })
    }
}

fn parse_encryption_key(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 || !value.is_ascii() {
        bail!("EME_CREDENTIAL_ENCRYPTION_KEY must contain exactly 64 hexadecimal characters");
    }
    let mut key = [0_u8; 32];
    for (index, byte) in key.iter_mut().enumerate() {
        let start = index * 2;
        *byte = u8::from_str_radix(&value[start..start + 2], 16)
            .context("EME_CREDENTIAL_ENCRYPTION_KEY must be hexadecimal")?;
    }
    Ok(key)
}
