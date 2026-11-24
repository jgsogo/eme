//! Agent tool adapters for media discovery, preferences and activity.
use std::str::FromStr;

use agent_runtime::{Tool, ToolContext, ToolDefinition, ToolError, ToolRegistry};
use agent_storage::{FamilyId, UserId};
use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    ContentRestrictions, FamilyMediaPreferences, MediaConfig, MediaKind, MediaProfile,
    MediaProfileId, MediaRating, MediaStore, MediaWatchEvent, ProfilePreferences,
    RecommendationContext, RecommendationCriteria, TmdbClient, TraktSyncService,
    WatchHistorySource, WatchmodeClient,
};

/// Dependencies shared by the registered media tools.
#[derive(Clone)]
pub struct MediaToolServices {
    store: MediaStore,
    config: MediaConfig,
    tmdb: Option<TmdbClient>,
    watchmode: Option<WatchmodeClient>,
    recommendations: Option<crate::RecommendationService>,
    sync: Option<TraktSyncService>,
}

impl MediaToolServices {
    pub fn new(
        store: MediaStore,
        config: MediaConfig,
        tmdb: Option<TmdbClient>,
        watchmode: Option<WatchmodeClient>,
        sync: Option<TraktSyncService>,
    ) -> Self {
        let recommendations = tmdb.as_ref().and_then(|tmdb| {
            crate::RecommendationService::new(
                store.clone(),
                tmdb.clone(),
                watchmode.clone(),
                config.clone(),
            )
            .ok()
        });
        Self {
            store,
            config,
            tmdb,
            watchmode,
            recommendations,
            sync,
        }
    }
}

/// Register the complete media tool surface in the agent's executable registry.
pub fn register_media_tools(
    registry: &mut ToolRegistry,
    services: MediaToolServices,
) -> Result<(), String> {
    let tools = [
        (
            "media_profiles",
            "List media profiles in the authenticated family. Use this to ask who is watching.",
            json!({
                "type":"object",
                "properties":{
                    "action":{"type":"string","enum":["list","create","update","delete"]},
                    "profile_id":{"type":"string"},
                    "name":{"type":"string","minLength":1},
                    "user_id":{"type":["string","null"]},
                    "active":{"type":"boolean"}
                },
                "additionalProperties":false
            }),
            MediaAction::Profiles,
        ),
        (
            "media_recommend",
            "Recommend movies and TV for explicitly selected family participants.",
            json!({
                "type":"object",
                "properties":{
                    "participant_ids":{"type":"array","items":{"type":"string"},"minItems":1},
                    "criteria":{"type":"object"},
                    "watched_policy":{"type":"string","enum":["any_participant","all_participants","include_all"]}
                },
                "required":["participant_ids"],
                "additionalProperties":false
            }),
            MediaAction::Recommend,
        ),
        (
            "media_search",
            "Search the TMDB catalogue for movies or TV shows.",
            json!({
                "type":"object",
                "properties":{
                    "query":{"type":"string","minLength":1},
                    "kind":{"type":"string","enum":["movie","tv_show"]}
                },
                "required":["query"],
                "additionalProperties":false
            }),
            MediaAction::Search,
        ),
        (
            "media_details",
            "Retrieve normalized TMDB details for a movie or TV show.",
            json!({
                "type":"object",
                "properties":{
                    "tmdb_id":{"type":"integer","minimum":1},
                    "kind":{"type":"string","enum":["movie","tv_show"]}
                },
                "required":["tmdb_id","kind"],
                "additionalProperties":false
            }),
            MediaAction::Details,
        ),
        (
            "media_availability",
            "Find Spanish streaming offers, including subscription, rent, buy, free and ad-supported options.",
            json!({
                "type":"object",
                "properties":{
                    "tmdb_id":{"type":"integer","minimum":1},
                    "kind":{"type":"string","enum":["movie","tv_show"]}
                },
                "required":["tmdb_id","kind"],
                "additionalProperties":false
            }),
            MediaAction::Availability,
        ),
        (
            "media_watch_history",
            "Read the authenticated user's own viewing history. Omit profile_id to use their personal media profile; only provide it for an explicitly selected family-managed profile.",
            json!({
                "type":"object",
                "properties":{"profile_id":{"type":"string"}},
                "additionalProperties":false
            }),
            MediaAction::History,
        ),
        (
            "media_watchlist",
            "Read the linked Trakt watchlist, or add/remove titles on the user's explicit request. Omit profile_id to use their personal media profile; writes require that profile's own Trakt connection.",
            json!({
                "type":"object",
                "properties":{
                    "action":{"type":"string","enum":["list","add","remove"]},
                    "profile_id":{"type":"string"},
                    "tmdb_id":{"type":"integer","minimum":1},
                    "kind":{"type":"string","enum":["movie","tv_show"]}
                },
                "additionalProperties":false
            }),
            MediaAction::Watchlist,
        ),
        (
            "media_rate",
            "Record a 0–5 rating on the user's explicit request and write it through to Trakt when the selected profile has the caller's Trakt account linked. Omit profile_id to use their personal media profile.",
            json!({
                "type":"object",
                "properties":{
                    "profile_id":{"type":"string"},
                    "tmdb_id":{"type":"integer","minimum":1},
                    "kind":{"type":"string","enum":["movie","tv_show"]},
                    "rating":{"type":"number","minimum":0,"maximum":5}
                },
                "required":["tmdb_id","kind","rating"],
                "additionalProperties":false
            }),
            MediaAction::Rate,
        ),
        (
            "media_preferences",
            "Read or update explicit family/profile preferences and hard content restrictions.",
            json!({
                "type":"object",
                "properties":{
                    "action":{"type":"string","enum":["get","update"]},
                    "scope":{"type":"string","enum":["family","profile"]},
                    "profile_id":{"type":"string"},
                    "preferences":{"type":"object"},
                    "restrictions":{"type":"object"}
                },
                "required":["action","scope"],
                "additionalProperties":false
            }),
            MediaAction::Preferences,
        ),
        (
            "media_sync_history",
            "Synchronize the authenticated user's personal Trakt profile. Omit profile_id to use their personal media profile; only provide it for an explicitly selected family-managed profile.",
            json!({
                "type":"object",
                "properties":{"profile_id":{"type":"string"}},
                "additionalProperties":false
            }),
            MediaAction::SyncHistory,
        ),
    ];

    for (name, description, parameters, action) in tools {
        registry
            .register(
                ToolDefinition {
                    name: name.into(),
                    description: description.into(),
                    parameters,
                },
                MediaTool {
                    services: services.clone(),
                    action,
                },
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum MediaAction {
    Profiles,
    Recommend,
    Search,
    Details,
    Availability,
    History,
    Watchlist,
    Rate,
    Preferences,
    SyncHistory,
}

struct MediaTool {
    services: MediaToolServices,
    action: MediaAction,
}

#[async_trait]
impl Tool for MediaTool {
    async fn execute(&self, context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let result = match self.action {
            MediaAction::Profiles => self.profiles(context, &arguments).await,
            MediaAction::Recommend => self.recommend(context, &arguments).await,
            MediaAction::Search => self.search(&arguments).await,
            MediaAction::Details => self.details(&arguments).await,
            MediaAction::Availability => self.availability(&arguments).await,
            MediaAction::History => self.history(context, &arguments).await,
            MediaAction::Watchlist => self.watchlist(context, &arguments).await,
            MediaAction::Rate => self.rate(context, &arguments).await,
            MediaAction::Preferences => self.preferences(context, &arguments).await,
            MediaAction::SyncHistory => self.sync_history(context, &arguments).await,
        };
        result
            .and_then(|value| serde_json::to_string(&value).map_err(|error| error.to_string()))
            .map_err(ToolError::Execution)
    }
}

impl MediaTool {
    async fn profiles(&self, context: &ToolContext, args: &Value) -> Result<Value, String> {
        let (_, family_id, role) = family_scope(context)?;
        match args.get("action").and_then(Value::as_str).unwrap_or("list") {
            "list" => {
                let profiles = self
                    .services
                    .store
                    .list_profiles(family_id)
                    .await
                    .map_err(display)?;
                serde_json::to_value(profiles).map_err(display)
            }
            "create" => {
                require_family_admin(role)?;
                let mut profile = MediaProfile::new(family_id, required_string(args, "name")?);
                if let Some(user_id) = args.get("user_id") {
                    profile.user_id = parse_profile_user(context, user_id)?;
                }
                self.services
                    .store
                    .save_profile(&profile)
                    .await
                    .map_err(display)?;
                serde_json::to_value(profile).map_err(display)
            }
            "update" => {
                require_family_admin(role)?;
                let profile_id = MediaProfileId::from_str(required_string(args, "profile_id")?)
                    .map_err(display)?;
                let mut profile = self
                    .services
                    .store
                    .get_profile(family_id, profile_id)
                    .await
                    .map_err(display)?
                    .ok_or_else(|| {
                        "profile does not belong to the authenticated family".to_owned()
                    })?;
                if let Some(name) = args.get("name").and_then(Value::as_str) {
                    profile.name = name.to_owned();
                }
                if let Some(active) = args.get("active").and_then(Value::as_bool) {
                    profile.active = active;
                }
                if let Some(user_id) = args.get("user_id") {
                    profile.user_id = parse_profile_user(context, user_id)?;
                }
                self.services
                    .store
                    .save_profile(&profile)
                    .await
                    .map_err(display)?;
                serde_json::to_value(profile).map_err(display)
            }
            "delete" => {
                require_family_admin(role)?;
                let profile_id = MediaProfileId::from_str(required_string(args, "profile_id")?)
                    .map_err(display)?;
                if self
                    .services
                    .store
                    .get_external_connection_link(profile_id, "trakt")
                    .await
                    .map_err(display)?
                    .is_some()
                {
                    return Err("unlink the profile's Trakt account before deleting it".into());
                }
                self.services
                    .store
                    .delete_profile(family_id, profile_id)
                    .await
                    .map_err(display)?;
                Ok(json!({"deleted_profile_id": profile_id.to_string()}))
            }
            _ => Err("action must be list, create, update or delete".into()),
        }
    }

    async fn recommend(&self, context: &ToolContext, args: &Value) -> Result<Value, String> {
        let (_, family_id, _) = family_scope(context)?;
        let service = self.services.recommendations.as_ref().ok_or_else(|| {
            "Media recommendations require configured TMDB and Watchmode API keys".to_owned()
        })?;
        let participant_ids = string_array(args, "participant_ids")?
            .iter()
            .map(|value| MediaProfileId::from_str(value).map_err(display))
            .collect::<Result<Vec<_>, _>>()?;
        let criteria = args
            .get("criteria")
            .cloned()
            .map(serde_json::from_value::<RecommendationCriteria>)
            .transpose()
            .map_err(display)?
            .unwrap_or_default();
        let watched_policy = args
            .get("watched_policy")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(display)?;
        let context = RecommendationContext {
            family_id,
            participants: participant_ids,
            criteria,
            watched_policy,
        };
        let recommendations = service.recommend(&context).await.map_err(display)?;
        serde_json::to_value(recommendations).map_err(display)
    }

    async fn search(&self, args: &Value) -> Result<Value, String> {
        let tmdb = self.tmdb()?;
        let query = required_string(args, "query")?;
        let page = match args.get("kind").and_then(Value::as_str) {
            Some("movie") => tmdb.search_movies(query, 1).await.map_err(display)?,
            Some("tv_show") => tmdb.search_tv(query, 1).await.map_err(display)?,
            Some(_) => return Err("kind must be movie or tv_show".into()),
            None => {
                let movies = tmdb.search_movies(query, 1).await.map_err(display)?;
                let shows = tmdb.search_tv(query, 1).await.map_err(display)?;
                let mut items = movies.items;
                items.extend(shows.items);
                return serde_json::to_value(items).map_err(display);
            }
        };
        serde_json::to_value(page.items).map_err(display)
    }

    async fn details(&self, args: &Value) -> Result<Value, String> {
        let tmdb = self.tmdb()?;
        let id = required_u64(args, "tmdb_id")?;
        let kind = parse_kind(required_string(args, "kind")?)?;
        let item = match kind {
            MediaKind::Movie => tmdb.movie_details(id).await,
            MediaKind::TvShow => tmdb.tv_details(id).await,
        }
        .map_err(display)?;
        serde_json::to_value(item).map_err(display)
    }

    async fn availability(&self, args: &Value) -> Result<Value, String> {
        let id = required_u64(args, "tmdb_id")?;
        let kind = parse_kind(required_string(args, "kind")?)?;
        let mut offers = if let Some(watchmode) = &self.services.watchmode {
            match watchmode
                .offers_for_tmdb_id(id, kind, &self.services.config.country)
                .await
            {
                Ok(offers) => offers,
                Err(_) => {
                    tracing::warn!(
                        provider = "watchmode",
                        "availability lookup failed; falling back to TMDB watch providers"
                    );
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        if offers.is_empty() {
            offers = self
                .tmdb()?
                .watch_providers(id, kind, &self.services.config.country)
                .await
                .map_err(display)?;
        }
        serde_json::to_value(offers).map_err(display)
    }

    async fn history(&self, context: &ToolContext, args: &Value) -> Result<Value, String> {
        let profile_id = self
            .require_profile(
                context,
                args.get("profile_id").and_then(Value::as_str),
                false,
            )
            .await?;
        let events: Vec<MediaWatchEvent> = self
            .services
            .store
            .list_watch_history(profile_id)
            .await
            .map_err(display)?;
        let mut history = Vec::with_capacity(events.len());
        for event in events {
            let item = self
                .services
                .store
                .get_media_item(event.media_item_id)
                .await
                .map_err(display)?;
            history.push(json!({
                "event": event,
                "media_item": item,
            }));
        }
        let sync_state = self
            .services
            .store
            .get_provider_sync_state(profile_id, "trakt")
            .await
            .map_err(display)?;
        Ok(json!({
            "profile_id": profile_id.to_string(),
            "events": history,
            "trakt_sync": sync_state,
        }))
    }

    async fn watchlist(&self, context: &ToolContext, args: &Value) -> Result<Value, String> {
        let action = args.get("action").and_then(Value::as_str).unwrap_or("list");
        let profile_id = self
            .require_profile(
                context,
                args.get("profile_id").and_then(Value::as_str),
                action != "list",
            )
            .await?;
        match action {
            "list" => {
                let items = self
                    .services
                    .store
                    .list_watchlist(profile_id)
                    .await
                    .map_err(display)?;
                serde_json::to_value(items).map_err(display)
            }
            "add" | "remove" => {
                let user_id = authenticated_user_id(context)?;
                let tmdb_id = required_u64(args, "tmdb_id")?;
                let kind = parse_kind(required_string(args, "kind")?)?;
                let item = match kind {
                    MediaKind::Movie => self.tmdb()?.movie_details(tmdb_id).await,
                    MediaKind::TvShow => self.tmdb()?.tv_details(tmdb_id).await,
                }
                .map_err(display)?;
                let sync =
                    self.services.sync.as_ref().ok_or_else(|| {
                        "Trakt write synchronization is not configured".to_owned()
                    })?;
                sync.set_watchlist_item(user_id, profile_id, &item, action == "add")
                    .await
                    .map_err(display)?;
                Ok(json!({
                    "action": action,
                    "profile_id": profile_id.to_string(),
                    "item": item
                }))
            }
            _ => Err("action must be list, add or remove".into()),
        }
    }

    async fn rate(&self, context: &ToolContext, args: &Value) -> Result<Value, String> {
        let profile_id = self
            .require_profile(
                context,
                args.get("profile_id").and_then(Value::as_str),
                true,
            )
            .await?;
        let user_id = authenticated_user_id(context)?;
        let tmdb_id = required_u64(args, "tmdb_id")?;
        let kind = parse_kind(required_string(args, "kind")?)?;
        let rating = args
            .get("rating")
            .and_then(Value::as_f64)
            .ok_or_else(|| "rating must be a number".to_owned())? as f32;
        if !rating.is_finite() || !(0.0..=5.0).contains(&rating) {
            return Err("rating must be between 0 and 5".into());
        }
        let item = match kind {
            MediaKind::Movie => self.tmdb()?.movie_details(tmdb_id).await,
            MediaKind::TvShow => self.tmdb()?.tv_details(tmdb_id).await,
        }
        .map_err(display)?;
        let record = MediaRating {
            profile_id,
            media_item_id: item.id,
            value: rating,
            rated_at: Utc::now(),
            source: WatchHistorySource::Eme,
        };

        let link = self
            .services
            .store
            .get_external_connection_link(profile_id, "trakt")
            .await
            .map_err(display)?;
        let trakt_synced = if let Some(link) = link {
            if link.owner_id != user_id {
                return Err(
                    "the linked Trakt account belongs to another profile owner; only that owner can change its Trakt ratings"
                        .into(),
                );
            }
            let sync = self
                .services
                .sync
                .as_ref()
                .ok_or_else(|| "Trakt write synchronization is not configured".to_owned())?;
            sync.push_rating(user_id, profile_id, &item, &record)
                .await
                .map_err(display)?;
            true
        } else {
            self.services
                .store
                .save_media_item(&item)
                .await
                .map_err(display)?;
            self.services
                .store
                .save_rating(&record)
                .await
                .map_err(display)?;
            false
        };
        Ok(json!({
            "rating": record,
            "trakt_synced": trakt_synced
        }))
    }

    async fn preferences(&self, context: &ToolContext, args: &Value) -> Result<Value, String> {
        let action = required_string(args, "action")?;
        let scope = required_string(args, "scope")?;
        let (_user_id, family_id, role) = family_scope(context)?;
        match (scope, action) {
            ("family", "get") => {
                let preferences = self
                    .services
                    .store
                    .get_family_preferences(family_id)
                    .await
                    .map_err(display)?
                    .unwrap_or_else(|| FamilyMediaPreferences::new(family_id));
                serde_json::to_value(preferences).map_err(display)
            }
            ("family", "update") => {
                require_family_admin(role)?;
                let value = args
                    .get("preferences")
                    .cloned()
                    .ok_or_else(|| "preferences are required for update".to_owned())?;
                let value = set_json_field(
                    value,
                    "family_id",
                    serde_json::to_value(family_id).map_err(display)?,
                )?;
                let mut preferences: FamilyMediaPreferences =
                    serde_json::from_value(value).map_err(display)?;
                preferences.family_id = family_id;
                self.services
                    .store
                    .save_family_preferences(&preferences)
                    .await
                    .map_err(display)?;
                serde_json::to_value(preferences).map_err(display)
            }
            ("profile", "get") => {
                let profile_id = self
                    .require_profile(context, Some(required_string(args, "profile_id")?), false)
                    .await?;
                let preferences = self
                    .services
                    .store
                    .get_profile_preferences(profile_id)
                    .await
                    .map_err(display)?
                    .unwrap_or_else(|| ProfilePreferences::new(profile_id));
                let restrictions = self
                    .services
                    .store
                    .get_content_restrictions(profile_id)
                    .await
                    .map_err(display)?
                    .unwrap_or_else(|| ContentRestrictions::new(profile_id));
                Ok(json!({"preferences": preferences, "restrictions": restrictions}))
            }
            ("profile", "update") => {
                let profile_id = self
                    .require_profile(context, Some(required_string(args, "profile_id")?), true)
                    .await?;
                let mut updated = false;
                if let Some(value) = args.get("preferences") {
                    let value = set_json_field(
                        value.clone(),
                        "profile_id",
                        serde_json::to_value(profile_id).map_err(display)?,
                    )?;
                    let mut preferences: ProfilePreferences =
                        serde_json::from_value(value).map_err(display)?;
                    preferences.profile_id = profile_id;
                    self.services
                        .store
                        .save_profile_preferences(&preferences)
                        .await
                        .map_err(display)?;
                    updated = true;
                }
                if let Some(value) = args.get("restrictions") {
                    let value = set_json_field(
                        value.clone(),
                        "profile_id",
                        serde_json::to_value(profile_id).map_err(display)?,
                    )?;
                    let mut restrictions: ContentRestrictions =
                        serde_json::from_value(value).map_err(display)?;
                    restrictions.profile_id = profile_id;
                    self.services
                        .store
                        .save_content_restrictions(&restrictions)
                        .await
                        .map_err(display)?;
                    updated = true;
                }
                if !updated {
                    return Err("preferences or restrictions are required for update".into());
                }
                let preferences = self
                    .services
                    .store
                    .get_profile_preferences(profile_id)
                    .await
                    .map_err(display)?;
                let restrictions = self
                    .services
                    .store
                    .get_content_restrictions(profile_id)
                    .await
                    .map_err(display)?;
                Ok(json!({"preferences": preferences, "restrictions": restrictions}))
            }
            _ => Err("scope must be family or profile, and action must be get or update".into()),
        }
    }

    async fn sync_history(&self, context: &ToolContext, args: &Value) -> Result<Value, String> {
        let profile_id = self
            .require_profile(
                context,
                args.get("profile_id").and_then(Value::as_str),
                false,
            )
            .await?;
        let user_id = authenticated_user_id(context)?;
        let sync = self
            .services
            .sync
            .as_ref()
            .ok_or_else(|| "Trakt synchronization is not configured".to_owned())?;
        let summary = sync
            .sync_profile(user_id, profile_id)
            .await
            .map_err(display)?;
        serde_json::to_value(summary).map_err(display)
    }

    async fn require_profile(
        &self,
        context: &ToolContext,
        profile_id: Option<&str>,
        manage: bool,
    ) -> Result<MediaProfileId, String> {
        let user_id = authenticated_user_id(context)?;
        let Some(profile_id) = profile_id else {
            return self
                .services
                .store
                .get_or_create_personal_profile(user_id)
                .await
                .map(|profile| profile.id)
                .map_err(display);
        };
        let profile_id = MediaProfileId::from_str(profile_id).map_err(display)?;

        if self
            .services
            .store
            .get_personal_profile(user_id, profile_id)
            .await
            .map_err(display)?
            .is_some()
        {
            return Ok(profile_id);
        }

        let family = context.family.as_ref().ok_or_else(|| {
            "this is a family-managed media profile; select its family first".to_owned()
        })?;
        let family_id = parse_id::<FamilyId>(&family.family_id)?;
        let profile: MediaProfile = self
            .services
            .store
            .get_profile(family_id, profile_id)
            .await
            .map_err(display)?
            .ok_or_else(|| {
                "media profile does not belong to the authenticated user or selected family"
                    .to_owned()
            })?;
        if manage && !is_family_admin(&family.family_role) && profile.user_id != Some(user_id) {
            return Err("you are not allowed to modify this media profile".into());
        }
        Ok(profile_id)
    }

    fn tmdb(&self) -> Result<&TmdbClient, String> {
        self.services
            .tmdb
            .as_ref()
            .ok_or_else(|| "TMDB is not configured".to_owned())
    }
}

fn authenticated_user_id(context: &ToolContext) -> Result<UserId, String> {
    context
        .user_id
        .as_deref()
        .ok_or_else(|| "authenticated user context is required".to_owned())
        .and_then(parse_id::<UserId>)
}

fn family_scope(context: &ToolContext) -> Result<(UserId, FamilyId, &str), String> {
    let user_id = authenticated_user_id(context)?;
    let family = context
        .family
        .as_ref()
        .ok_or_else(|| "an authenticated family context is required".to_owned())?;
    let family_id = parse_id::<FamilyId>(&family.family_id)?;
    Ok((user_id, family_id, family.family_role.as_str()))
}

fn parse_profile_user(context: &ToolContext, value: &Value) -> Result<Option<UserId>, String> {
    if value.is_null() {
        return Ok(None);
    }
    let user_id: UserId = serde_json::from_value(value.clone()).map_err(display)?;
    let family = context
        .family
        .as_ref()
        .ok_or_else(|| "an authenticated family context is required".to_owned())?;
    if !family
        .members
        .iter()
        .any(|member| member.user_id == user_id.to_string())
    {
        return Err("associated user must be an active member of this family".into());
    }
    Ok(Some(user_id))
}

fn parse_id<T: for<'de> Deserialize<'de>>(value: &str) -> Result<T, String> {
    serde_json::from_value(Value::String(value.to_owned())).map_err(display)
}

fn require_family_admin(role: &str) -> Result<(), String> {
    if is_family_admin(role) {
        Ok(())
    } else {
        Err("family owner or admin permission is required".into())
    }
}

fn is_family_admin(role: &str) -> bool {
    role.eq_ignore_ascii_case("owner") || role.eq_ignore_ascii_case("admin")
}

fn required_string<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} must be a non-empty string"))
}

fn required_u64(args: &Value, name: &str) -> Result<u64, String> {
    args.get(name)
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{name} must be a positive integer"))
}

fn string_array<'a>(args: &'a Value, name: &str) -> Result<Vec<&'a str>, String> {
    args.get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{name} must be an array"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| format!("{name} must contain non-empty strings"))
        })
        .collect()
}

fn set_json_field(mut value: Value, field: &str, field_value: Value) -> Result<Value, String> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| "preferences and restrictions must be JSON objects".to_owned())?;
    object.insert(field.to_owned(), field_value);
    Ok(value)
}

fn parse_kind(value: &str) -> Result<MediaKind, String> {
    match value {
        "movie" => Ok(MediaKind::Movie),
        "tv_show" => Ok(MediaKind::TvShow),
        _ => Err("kind must be movie or tv_show".into()),
    }
}

fn display(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use agent_runtime::{FamilyContext, ToolDispatcher};
    use agent_storage::{FamilyId, UserId};

    use super::*;

    #[tokio::test]
    async fn personal_history_defaults_to_authenticated_users_profile_without_family() {
        let store = MediaStore::open_in_memory().unwrap();
        let user_id = UserId::new();
        let services =
            MediaToolServices::new(store.clone(), MediaConfig::default(), None, None, None);
        let mut registry = ToolRegistry::new();
        register_media_tools(&mut registry, services).unwrap();
        let context = ToolContext::new(user_id.to_string());

        let output = registry
            .execute(&context, "media_watch_history", json!({}))
            .await
            .unwrap();
        let history: Value = serde_json::from_str(&output).unwrap();
        let profile = store.get_or_create_personal_profile(user_id).await.unwrap();

        assert_eq!(history["profile_id"], profile.id.to_string());
        assert_eq!(history["events"], json!([]));
        assert!(history["trakt_sync"].is_null());

        let watchlist = registry
            .execute(&context, "media_watchlist", json!({}))
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&watchlist).unwrap(),
            json!([])
        );
    }

    #[tokio::test]
    async fn registers_media_tools_and_uses_trusted_family_context() {
        let store = MediaStore::open_in_memory().unwrap();
        let family_id = FamilyId::new();
        let user_id = UserId::new();
        let mut profile = MediaProfile::new(family_id, "Child");
        profile.user_id = Some(user_id);
        store.save_profile(&profile).await.unwrap();

        let services =
            MediaToolServices::new(store.clone(), MediaConfig::default(), None, None, None);
        let mut registry = ToolRegistry::new();
        register_media_tools(&mut registry, services).unwrap();

        for name in [
            "media_profiles",
            "media_recommend",
            "media_search",
            "media_details",
            "media_availability",
            "media_watch_history",
            "media_watchlist",
            "media_rate",
            "media_preferences",
            "media_sync_history",
        ] {
            assert!(registry.contains(name), "missing tool {name}");
        }

        let context = ToolContext {
            user_id: Some(user_id.to_string()),
            family: Some(FamilyContext {
                family_id: family_id.to_string(),
                family_name: "Home".into(),
                family_role: "member".into(),
                relationship: "parent".into(),
                members: Vec::new(),
            }),
            ..ToolContext::default()
        };
        let output = registry
            .execute(&context, "media_profiles", json!({}))
            .await
            .unwrap();
        let profiles: Vec<MediaProfile> = serde_json::from_str(&output).unwrap();
        assert_eq!(profiles, vec![profile.clone()]);

        let error = registry
            .execute(
                &context,
                "media_recommend",
                json!({
                    "participant_ids": [profile.id.to_string()],
                    "family_id": FamilyId::new().to_string()
                }),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, ToolError::InvalidArguments { .. }));
    }

    #[tokio::test]
    async fn recommendation_tool_executes_the_provider_pipeline_for_selected_profiles() {
        use std::time::Duration;

        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path, query_param},
        };

        let tmdb_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/genre/movie/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "genres": [{"id": 35, "name": "Comedy"}]
            })))
            .mount(&tmdb_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/search/movie"))
            .and(query_param("query", "comedy"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "page": 1,
                "total_pages": 1,
                "total_results": 1,
                "results": [{
                    "id": 42,
                    "title": "Tool Recommendation",
                    "genre_ids": [35],
                    "release_date": "2025-01-01",
                    "popularity": 30.0,
                    "vote_average": 8.0,
                    "vote_count": 500
                }]
            })))
            .mount(&tmdb_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/movie/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": 42,
                "title": "Tool Recommendation",
                "release_date": "2025-01-01",
                "genres": [{"id": 35, "name": "Comedy"}],
                "runtime": 95,
                "vote_average": 8.0,
                "vote_count": 500,
                "release_dates": {
                    "results": [{
                        "iso_3166_1": "ES",
                        "release_dates": [{"certification": "7"}]
                    }]
                }
            })))
            .mount(&tmdb_server)
            .await;

        let watchmode_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search/"))
            .and(query_param("search_value", "42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "title_results": [{"id": 9001}]
            })))
            .mount(&watchmode_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/title/9001/sources/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"source_id": 203, "name": "Netflix", "type": "sub", "region": "ES", "web_url": "https://example.invalid/title"}
            ])))
            .mount(&watchmode_server)
            .await;

        let store = MediaStore::open_in_memory().unwrap();
        let family_id = FamilyId::new();
        let profile = MediaProfile::new(family_id, "Selected viewer");
        store.save_profile(&profile).await.unwrap();
        let tmdb = TmdbClient::with_base_url("tmdb-key", "es-ES", "ES", tmdb_server.uri()).unwrap();
        let watchmode = WatchmodeClient::with_options(
            "watchmode-key",
            watchmode_server.uri(),
            Duration::from_secs(60),
        )
        .unwrap();
        let services = MediaToolServices::new(
            store,
            MediaConfig::default(),
            Some(tmdb),
            Some(watchmode),
            None,
        );
        let mut registry = ToolRegistry::new();
        register_media_tools(&mut registry, services).unwrap();
        let context = ToolContext {
            user_id: Some(UserId::new().to_string()),
            family: Some(FamilyContext {
                family_id: family_id.to_string(),
                family_name: "Home".into(),
                family_role: "member".into(),
                relationship: "parent".into(),
                members: Vec::new(),
            }),
            ..ToolContext::default()
        };

        let output = registry
            .execute(
                &context,
                "media_recommend",
                json!({
                    "participant_ids": [profile.id.to_string()],
                    "criteria": {
                        "kinds": ["movie"],
                        "query": "comedy"
                    }
                }),
            )
            .await
            .unwrap();
        let recommendations: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(recommendations.as_array().unwrap().len(), 1);
        assert_eq!(recommendations[0]["offers"][0]["service_name"], "Netflix");
        assert_eq!(
            recommendations[0]["history_unknown_for"][0],
            json!(profile.id.to_string())
        );
    }

    #[tokio::test]
    async fn preference_updates_bind_ids_to_authenticated_family_and_profile() {
        let store = MediaStore::open_in_memory().unwrap();
        let family_id = FamilyId::new();
        let user_id = UserId::new();
        let mut profile = MediaProfile::new(family_id, "Viewer");
        profile.user_id = Some(user_id);
        store.save_profile(&profile).await.unwrap();
        let services =
            MediaToolServices::new(store.clone(), MediaConfig::default(), None, None, None);
        let mut registry = ToolRegistry::new();
        register_media_tools(&mut registry, services).unwrap();
        let context = ToolContext {
            user_id: Some(user_id.to_string()),
            family: Some(FamilyContext {
                family_id: family_id.to_string(),
                family_name: "Home".into(),
                family_role: "member".into(),
                relationship: "parent".into(),
                members: Vec::new(),
            }),
            ..ToolContext::default()
        };

        registry
            .execute(
                &context,
                "media_preferences",
                json!({
                    "action": "update",
                    "scope": "profile",
                    "profile_id": profile.id.to_string(),
                    "preferences": {
                        "profile_id": MediaProfileId::new().to_string(),
                        "liked_genres": ["Comedy"]
                    }
                }),
            )
            .await
            .unwrap();

        let preferences = store
            .get_profile_preferences(profile.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(preferences.profile_id, profile.id);
        assert_eq!(preferences.liked_genres, vec!["Comedy"]);
    }
}
