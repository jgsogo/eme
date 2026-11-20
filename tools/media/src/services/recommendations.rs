//! Deterministic family-aware recommendation pipeline.
use std::{collections::{HashMap, HashSet}, time::Instant};

use chrono::Datelike;

use crate::{
    ContentRestrictions, FamilyMediaPreferences, MediaConfig, MediaItem, MediaItemId, MediaKind,
    MediaProfile, MediaProfileId, MediaRating, MediaStore, MediaStoreError, MediaWatchEvent,
    ProfilePreferences, RecommendationContext, RecommendationCriteria, StreamingOffer, TmdbClient,
    TmdbError, WatchedPolicy, WatchmodeClient, WatchmodeError,
};

/// One ranked candidate with the evidence used to recommend it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MediaRecommendation {
    pub item: MediaItem,
    /// Deterministic weighted score; higher is better.
    pub score: i32,
    pub reasons: Vec<String>,
    pub offers: Vec<StreamingOffer>,
    pub watched_by: Vec<MediaProfileId>,
    /// Profiles whose history is incomplete, so watched exclusion is uncertain.
    pub history_unknown_for: Vec<MediaProfileId>,
}

#[derive(Debug, thiserror::Error)]
pub enum RecommendationError {
    #[error("invalid media configuration: {0}")]
    Configuration(String),
    #[error("recommendation context must include at least one participant")]
    NoParticipants,
    #[error("profile {0} is not an active member of the requested family")]
    InvalidParticipant(MediaProfileId),
    #[error("subscription filtering is enabled but Watchmode is not configured")]
    MissingAvailabilityProvider,
    #[error("catalogue request failed: {0}")]
    Catalogue(#[from] TmdbError),
    #[error("availability request failed: {0}")]
    Availability(#[from] WatchmodeError),
    #[error("media storage failed: {0}")]
    Storage(#[from] MediaStoreError),
}

/// Deterministic discovery, filtering and ranking over the configured providers.
#[derive(Clone)]
pub struct RecommendationService {
    media: MediaStore,
    tmdb: TmdbClient,
    watchmode: Option<WatchmodeClient>,
    config: MediaConfig,
    candidate_limit_per_kind: usize,
}

impl RecommendationService {
    pub fn new(
        media: MediaStore,
        tmdb: TmdbClient,
        watchmode: Option<WatchmodeClient>,
        config: MediaConfig,
    ) -> Result<Self, RecommendationError> {
        config
            .validate()
            .map_err(|error| RecommendationError::Configuration(error.to_string()))?;
        if config.require_subscription && watchmode.is_none() {
            return Err(RecommendationError::MissingAvailabilityProvider);
        }
        Ok(Self {
            media,
            tmdb,
            watchmode,
            config,
            candidate_limit_per_kind: 20,
        })
    }

    /// Recommend for exactly the participants supplied in the validated context.
    ///
    /// Shared family preferences always contribute. Individual preferences,
    /// restrictions and activity are loaded only for selected participants.
    /// The caller must resolve and authorize the family ID before calling.
    pub async fn recommend(
        &self,
        context: &RecommendationContext,
    ) -> Result<Vec<MediaRecommendation>, RecommendationError> {
        let started = Instant::now();
        tracing::debug!(
            participant_count = context.participants.len(),
            "starting media recommendation"
        );
        if context.participants.is_empty() {
            return Err(RecommendationError::NoParticipants);
        }
        let mut seen = HashSet::new();
        if context.participants.iter().any(|id| !seen.insert(*id)) {
            return Err(RecommendationError::NoParticipants);
        }

        let family_preferences = self
            .media
            .get_family_preferences(context.family_id)
            .await?
            .unwrap_or_else(|| FamilyMediaPreferences::new(context.family_id));
        let mut participants = Vec::with_capacity(context.participants.len());
        for profile_id in &context.participants {
            let profile = self
                .media
                .get_profile(context.family_id, *profile_id)
                .await?
                .filter(|profile| profile.active)
                .ok_or(RecommendationError::InvalidParticipant(*profile_id))?;
            participants.push(self.load_participant(profile).await?);
        }

        let candidates = self.discover_candidates(&context.criteria).await?;
        let discovered_count = candidates.len();
        let watched_policy = context.watched_policy.unwrap_or(self.config.watched_policy);

        let mut recommendations = Vec::new();
        let mut excluded_by_criteria = 0;
        let mut excluded_by_restrictions = 0;
        let mut excluded_as_watched = 0;
        let mut excluded_by_availability = 0;
        for candidate in candidates {
            let item = self.enrich(candidate).await?;
            if !matches_criteria(&item, &context.criteria) {
                excluded_by_criteria += 1;
                continue;
            }
            if !participants
                .iter()
                .all(|participant| passes_restrictions(&item, &participant.restrictions))
            {
                excluded_by_restrictions += 1;
                continue;
            }

            let (watched_by, history_unknown_for) = watched_evidence(item.id, &participants);
            if should_exclude_watched(watched_policy, participants.len(), watched_by.len()) {
                excluded_as_watched += 1;
                continue;
            }

            let offers = self.availability(&item).await?;
            if self.config.require_subscription && offers.is_empty() {
                excluded_by_availability += 1;
                continue;
            }

            let (score, mut reasons) = score_item(&item, &family_preferences, &participants);
            for offer in &offers {
                reasons.push(format!("Available on {}", offer.service_name));
            }
            if !history_unknown_for.is_empty() {
                reasons.push(format!(
                    "Viewing history is incomplete for {} selected participant(s)",
                    history_unknown_for.len()
                ));
            }
            recommendations.push(MediaRecommendation {
                item,
                score: score.round() as i32,
                reasons,
                offers,
                watched_by,
                history_unknown_for,
            });
        }

        let eligible_count = recommendations.len();
        let recommendations =
            apply_diversity(recommendations, self.config.recommendation_limit);
        let uncertain_profile_count = recommendations
            .iter()
            .map(|recommendation| recommendation.history_unknown_for.len())
            .sum::<usize>();
        tracing::info!(
            discovered = discovered_count,
            eligible = eligible_count,
            returned = recommendations.len(),
            excluded_by_criteria,
            excluded_by_restrictions,
            excluded_as_watched,
            excluded_by_availability,
            history_uncertain_profiles = uncertain_profile_count,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "media recommendation completed"
        );
        Ok(recommendations)
    }

    async fn load_participant(
        &self,
        profile: MediaProfile,
    ) -> Result<ParticipantData, RecommendationError> {
        let preferences = self
            .media
            .get_profile_preferences(profile.id)
            .await?
            .unwrap_or_else(|| ProfilePreferences::new(profile.id));
        let restrictions = self
            .media
            .get_content_restrictions(profile.id)
            .await?
            .unwrap_or_else(|| ContentRestrictions::new(profile.id));
        let history = self.media.list_watch_history(profile.id).await?;
        let watched_items = history
            .into_iter()
            .map(|event: MediaWatchEvent| event.media_item_id)
            .collect();
        let history_complete = self
            .media
            .get_provider_sync_state(profile.id, "trakt")
            .await?
            .is_some_and(|state| state.status == "complete");
        let ratings = self.media.list_ratings(profile.id).await?;
        let genre_affinity = self.genre_affinity(&ratings).await?;
        Ok(ParticipantData {
            profile,
            preferences,
            restrictions,
            watched_items,
            history_complete,
            genre_affinity,
        })
    }

    async fn genre_affinity(
        &self,
        ratings: &[MediaRating],
    ) -> Result<HashMap<String, f32>, RecommendationError> {
        let mut totals: HashMap<String, (f32, usize)> = HashMap::new();
        // Infer genre affinity dynamically from recent ratings. Trakt's sync
        // payload is intentionally compact, so enrich rated items from TMDB
        // when their local catalogue metadata does not yet include genres.
        for rating in ratings.iter().take(100) {
            let Some(mut item) = self.media.get_media_item(rating.media_item_id).await? else {
                continue;
            };
            if item.genres.is_empty() {
                if let Some(tmdb_id) = item
                    .external_ids
                    .iter()
                    .find(|id| id.provider == "tmdb")
                    .and_then(|id| id.value.parse::<u64>().ok())
                {
                    let enriched = match item.kind {
                        MediaKind::Movie => self.tmdb.movie_details(tmdb_id).await,
                        MediaKind::TvShow => self.tmdb.tv_details(tmdb_id).await,
                    };
                    if let Ok(enriched) = enriched {
                        self.media.save_media_item(&enriched).await?;
                        item = enriched;
                    }
                }
            }
            for genre in item.genres {
                let entry = totals.entry(normalize(&genre)).or_default();
                entry.0 += rating.value;
                entry.1 += 1;
            }
        }
        Ok(totals
            .into_iter()
            .filter_map(|(genre, (total, count))| {
                (count > 0).then_some((genre, total / count as f32))
            })
            .collect())
    }

    async fn discover_candidates(
        &self,
        criteria: &RecommendationCriteria,
    ) -> Result<Vec<MediaItem>, RecommendationError> {
        let mut seen = HashSet::new();
        let mut items = Vec::new();
        let query = criteria
            .query
            .as_deref()
            .filter(|query| !query.trim().is_empty());
        for kind in requested_kinds(criteria) {
            let page = match (kind, query) {
                (MediaKind::Movie, Some(query)) => self.tmdb.search_movies(query, 1).await?,
                (MediaKind::TvShow, Some(query)) => self.tmdb.search_tv(query, 1).await?,
                (MediaKind::Movie, None) => self.tmdb.popular_movies(1).await?,
                (MediaKind::TvShow, None) => self.tmdb.popular_tv(1).await?,
            };
            for item in page.items.into_iter().take(self.candidate_limit_per_kind) {
                if seen.insert(item.id) {
                    items.push(item);
                }
            }
        }
        Ok(items)
    }

    async fn enrich(&self, candidate: MediaItem) -> Result<MediaItem, RecommendationError> {
        let tmdb_id = candidate
            .external_ids
            .iter()
            .find(|id| id.provider == "tmdb")
            .and_then(|id| id.value.parse::<u64>().ok());
        let Some(tmdb_id) = tmdb_id else {
            return Ok(candidate);
        };
        Ok(match candidate.kind {
            MediaKind::Movie => self.tmdb.movie_details(tmdb_id).await?,
            MediaKind::TvShow => self.tmdb.tv_details(tmdb_id).await?,
        })
    }

    async fn availability(
        &self,
        item: &MediaItem,
    ) -> Result<Vec<StreamingOffer>, RecommendationError> {
        let Some(watchmode) = &self.watchmode else {
            return Ok(Vec::new());
        };
        let Some(tmdb_id) = item
            .external_ids
            .iter()
            .find(|id| id.provider == "tmdb")
            .and_then(|id| id.value.parse::<u64>().ok())
        else {
            return Ok(Vec::new());
        };
        let offers = watchmode
            .offers_for_tmdb_id(tmdb_id, item.kind, &self.config.country)
            .await?;
        Ok(crate::filter_subscription_offers(
            offers,
            &self.config.subscriptions,
        ))
    }
}

struct ParticipantData {
    profile: MediaProfile,
    preferences: ProfilePreferences,
    restrictions: ContentRestrictions,
    watched_items: HashSet<MediaItemId>,
    history_complete: bool,
    genre_affinity: HashMap<String, f32>,
}

fn requested_kinds(criteria: &RecommendationCriteria) -> Vec<MediaKind> {
    if criteria.kinds.is_empty() {
        vec![MediaKind::Movie, MediaKind::TvShow]
    } else {
        let mut kinds = Vec::new();
        for kind in &criteria.kinds {
            if !kinds.contains(kind) {
                kinds.push(*kind);
            }
        }
        kinds
    }
}

fn matches_criteria(item: &MediaItem, criteria: &RecommendationCriteria) -> bool {
    if !criteria.kinds.is_empty() && !criteria.kinds.contains(&item.kind) {
        return false;
    }
    if !criteria.genres.is_empty()
        && !criteria.genres.iter().any(|genre| {
            item.genres
                .iter()
                .any(|item_genre| normalize(item_genre) == normalize(genre))
        })
    {
        return false;
    }
    if !criteria.languages.is_empty()
        && !item.original_language.as_ref().is_some_and(|language| {
            criteria
                .languages
                .iter()
                .any(|candidate| normalize(candidate) == normalize(language))
        })
    {
        return false;
    }
    if let Some(maximum_runtime) = criteria.maximum_runtime_minutes
        && !item
            .runtime_minutes
            .is_some_and(|runtime| runtime <= maximum_runtime)
    {
        return false;
    }
    true
}

fn passes_restrictions(item: &MediaItem, restrictions: &ContentRestrictions) -> bool {
    if restrictions.disallowed_genres.iter().any(|genre| {
        item.genres
            .iter()
            .any(|item_genre| normalize(item_genre) == normalize(genre))
    }) {
        return false;
    }
    let Some(maximum) = restrictions.maximum_certification.as_deref() else {
        return true;
    };
    let Some(certification) = item.certification.as_deref() else {
        return restrictions.allow_unrated;
    };
    match (
        certification_rank(certification),
        certification_rank(maximum),
    ) {
        (Some(actual), Some(maximum)) => actual <= maximum,
        _ => restrictions.allow_unrated,
    }
}

fn certification_rank(value: &str) -> Option<u8> {
    match normalize(value).as_str() {
        "tp" | "0" | "g" => Some(0),
        "7" | "pg" => Some(1),
        "12" | "pg-13" => Some(2),
        "16" | "r" => Some(3),
        "18" | "nc-17" => Some(4),
        _ => None,
    }
}

fn watched_evidence(
    item_id: MediaItemId,
    participants: &[ParticipantData],
) -> (Vec<MediaProfileId>, Vec<MediaProfileId>) {
    let watched_by = participants
        .iter()
        .filter(|participant| participant.watched_items.contains(&item_id))
        .map(|participant| participant.profile.id)
        .collect();
    let unknown = participants
        .iter()
        .filter(|participant| !participant.history_complete)
        .map(|participant| participant.profile.id)
        .collect();
    (watched_by, unknown)
}

fn should_exclude_watched(
    policy: WatchedPolicy,
    participant_count: usize,
    watched_count: usize,
) -> bool {
    match policy {
        WatchedPolicy::AnyParticipant => watched_count > 0,
        WatchedPolicy::AllParticipants => {
            participant_count > 0 && watched_count == participant_count
        }
        WatchedPolicy::IncludeAll => false,
    }
}

fn score_item(
    item: &MediaItem,
    family: &FamilyMediaPreferences,
    participants: &[ParticipantData],
) -> (f32, Vec<String>) {
    let mut score = item.vote_average.unwrap_or(5.0) * 100.0;
    let mut reasons = Vec::new();
    let genres = item
        .genres
        .iter()
        .map(|genre| normalize(genre))
        .collect::<HashSet<_>>();

    for genre in &family.liked_genres {
        if genres.contains(&normalize(genre)) {
            score += 100.0;
            reasons.push(format!("Matches shared family preference: {genre}"));
        }
    }
    for genre in &family.disliked_genres {
        if genres.contains(&normalize(genre)) {
            score -= 100.0;
        }
    }
    if item.original_language.as_ref().is_some_and(|language| {
        family
            .preferred_languages
            .iter()
            .any(|value| normalize(value) == normalize(language))
    }) {
        score += 40.0;
        reasons.push("Matches a shared family language preference".into());
    }

    for participant in participants {
        for genre in &participant.preferences.liked_genres {
            if genres.contains(&normalize(genre)) {
                score += 60.0;
                reasons.push(format!(
                    "Matches {}'s preference: {genre}",
                    participant.profile.name
                ));
            }
        }
        for genre in &participant.preferences.disliked_genres {
            if genres.contains(&normalize(genre)) {
                score -= 60.0;
            }
        }
        if item.original_language.as_ref().is_some_and(|language| {
            participant
                .preferences
                .preferred_languages
                .iter()
                .any(|value| normalize(value) == normalize(language))
        }) {
            score += 30.0;
        }
        for genre in &genres {
            if let Some(average) = participant.genre_affinity.get(genre) {
                score += (average - 2.5) * 30.0;
            }
        }
    }

    if item.vote_average.is_some_and(|rating| rating >= 7.5)
        && item.vote_count.is_some_and(|count| count >= 100)
    {
        score += 40.0;
        reasons.push("Strong TMDB audience rating".into());
    }
    if let Some(popularity) = item.popularity.filter(|popularity| *popularity > 0.0) {
        score += popularity.log10().max(0.0) * 10.0;
    }
    if let Some(release_date) = item.release_date {
        let age = chrono::Utc::now().year() - release_date.year();
        if (0..=2).contains(&age) {
            score += 30.0;
            reasons.push("Recent release".into());
        } else if (0..=5).contains(&age) {
            score += 15.0;
        }
    }
    (score, reasons)
}

fn apply_diversity(
    mut recommendations: Vec<MediaRecommendation>,
    limit: usize,
) -> Vec<MediaRecommendation> {
    recommendations.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.item.title.cmp(&right.item.title))
    });
    let mut selected = Vec::new();
    let mut used_genres = HashSet::new();
    while !recommendations.is_empty() && selected.len() < limit {
        let (best_index, _) = recommendations
            .iter()
            .enumerate()
            .map(|(index, recommendation)| {
                let overlap = recommendation
                    .item
                    .genres
                    .iter()
                    .filter(|genre| used_genres.contains(&normalize(genre)))
                    .count() as i32;
                (index, recommendation.score - overlap * 25)
            })
            .max_by(|left, right| {
                left.1.cmp(&right.1).then_with(|| {
                    recommendations[right.0]
                        .item
                        .title
                        .cmp(&recommendations[left.0].item.title)
                })
            })
            .unwrap();
        let mut recommendation = recommendations.remove(best_index);
        let overlap = recommendation
            .item
            .genres
            .iter()
            .filter(|genre| used_genres.contains(&normalize(genre)))
            .count() as i32;
        recommendation.score -= overlap * 25;
        used_genres.extend(
            recommendation
                .item
                .genres
                .iter()
                .map(|genre| normalize(genre)),
        );
        selected.push(recommendation);
    }
    selected
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> MediaItem {
        MediaItem {
            id: MediaItemId::from_external_id("tmdb", "1"),
            kind: MediaKind::Movie,
            title: "Example".into(),
            original_title: None,
            overview: None,
            release_date: None,
            original_language: Some("en".into()),
            genres: vec!["Comedy".into()],
            runtime_minutes: Some(100),
            certification: Some("12".into()),
            popularity: Some(50.0),
            vote_average: Some(8.0),
            vote_count: Some(500),
            external_ids: vec![],
        }
    }

    #[tokio::test]
    async fn recommendation_e2e_uses_only_selected_profiles_and_requires_subscription_offer() {
        use std::time::Duration;

        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path, query_param},
        };

        let tmdb_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/genre/movie/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "genres": [{"id": 35, "name": "Comedy"}]
            })))
            .mount(&tmdb_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/search/movie"))
            .and(query_param("query", "comedy"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "page": 1,
                "total_pages": 1,
                "total_results": 1,
                "results": [{
                    "id": 42,
                    "title": "Family Comedy",
                    "genre_ids": [35],
                    "original_language": "en",
                    "release_date": "2025-01-01",
                    "popularity": 50.0,
                    "vote_average": 8.0,
                    "vote_count": 500
                }]
            })))
            .mount(&tmdb_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/movie/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 42,
                "title": "Family Comedy",
                "release_date": "2025-01-01",
                "original_language": "en",
                "genres": [{"id": 35, "name": "Comedy"}],
                "runtime": 100,
                "popularity": 50.0,
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
            .and(query_param("search_field", "tmdb_movie_id"))
            .and(query_param("search_value", "42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "title_results": [{"id": 9001}]
            })))
            .mount(&watchmode_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/title/9001/sources/"))
            .and(query_param("regions", "ES"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"source_id": 203, "name": "Netflix", "type": "sub", "region": "ES", "web_url": "https://example.invalid/title"}
            ])))
            .mount(&watchmode_server)
            .await;

        let media = MediaStore::open_in_memory().unwrap();
        let family_id = agent_storage::FamilyId::new();
        let selected = MediaProfile::new(family_id, "Selected viewer");
        let non_selected = MediaProfile::new(family_id, "Not watching");
        media.save_profile(&selected).await.unwrap();
        media.save_profile(&non_selected).await.unwrap();

        let mut family_preferences = FamilyMediaPreferences::new(family_id);
        family_preferences.liked_genres.push("Comedy".into());
        media
            .save_family_preferences(&family_preferences)
            .await
            .unwrap();

        let mut non_selected_preferences = ProfilePreferences::new(non_selected.id);
        non_selected_preferences.disliked_genres.push("Comedy".into());
        media
            .save_profile_preferences(&non_selected_preferences)
            .await
            .unwrap();

        let candidate_id = MediaItemId::from_external_id("tmdb", "42");
        let non_selected_watch = MediaWatchEvent {
            profile_id: non_selected.id,
            media_item_id: candidate_id,
            watched_at: chrono::Utc::now(),
            source: WatchHistorySource::Eme,
        };
        let minimal_item = MediaItem {
            id: candidate_id,
            kind: MediaKind::Movie,
            title: "Family Comedy".into(),
            original_title: None,
            overview: None,
            release_date: None,
            original_language: None,
            genres: Vec::new(),
            runtime_minutes: None,
            certification: None,
            popularity: None,
            vote_average: None,
            vote_count: None,
            external_ids: vec![],
        };
        media.save_media_item(&minimal_item).await.unwrap();
        media.record_watch_event(&non_selected_watch).await.unwrap();

        let tmdb = TmdbClient::with_base_url(
            "test-tmdb-key",
            "es-ES",
            "ES",
            tmdb_server.uri(),
        )
        .unwrap();
        let watchmode = WatchmodeClient::with_options(
            "test-watchmode-key",
            watchmode_server.uri(),
            Duration::from_secs(60),
        )
        .unwrap();
        let service = RecommendationService::new(
            media,
            tmdb,
            Some(watchmode),
            MediaConfig::default(),
        )
        .unwrap();

        let recommendations = service
            .recommend(&RecommendationContext {
                family_id,
                participants: vec![selected.id],
                criteria: RecommendationCriteria {
                    kinds: vec![MediaKind::Movie],
                    genres: vec!["Comedy".into()],
                    query: Some("comedy".into()),
                    ..RecommendationCriteria::default()
                },
                watched_policy: None,
            })
            .await
            .unwrap();

        assert_eq!(recommendations.len(), 1);
        let recommendation = &recommendations[0];
        assert_eq!(recommendation.item.title, "Family Comedy");
        assert!(recommendation.watched_by.is_empty());
        assert_eq!(recommendation.history_unknown_for, vec![selected.id]);
        assert_eq!(recommendation.offers.len(), 1);
        assert_eq!(recommendation.offers[0].service_name, "Netflix");
        assert!(recommendation.score >= 950);
        assert!(recommendation
            .reasons
            .iter()
            .any(|reason| reason.contains("shared family")));
        assert!(!recommendation
            .reasons
            .iter()
            .any(|reason| reason.contains("Not watching")));
    }

    #[test]
    fn hard_restrictions_are_applied_before_ranking() {
        let item = item();
        let mut restrictions = ContentRestrictions::new(MediaProfileId::new());
        restrictions.disallowed_genres.push("Comedy".into());
        assert!(!passes_restrictions(&item, &restrictions));

        restrictions.disallowed_genres.clear();
        restrictions.maximum_certification = Some("7".into());
        assert!(!passes_restrictions(&item, &restrictions));
        restrictions.maximum_certification = Some("16".into());
        assert!(passes_restrictions(&item, &restrictions));
    }

    #[test]
    fn unknown_certification_is_not_silently_treated_as_safe() {
        let mut item = item();
        item.certification = None;
        let mut restrictions = ContentRestrictions::new(MediaProfileId::new());
        restrictions.maximum_certification = Some("12".into());
        assert!(!passes_restrictions(&item, &restrictions));
        restrictions.allow_unrated = true;
        assert!(passes_restrictions(&item, &restrictions));
    }

    #[test]
    fn watched_policies_have_distinct_semantics() {
        assert!(should_exclude_watched(WatchedPolicy::AnyParticipant, 3, 1));
        assert!(!should_exclude_watched(
            WatchedPolicy::AllParticipants,
            3,
            1
        ));
        assert!(should_exclude_watched(WatchedPolicy::AllParticipants, 3, 3));
        assert!(!should_exclude_watched(WatchedPolicy::IncludeAll, 3, 3));
    }

    #[test]
    fn unknown_history_is_reported_but_not_mistaken_for_unwatched_or_watched() {
        let profile_id = MediaProfileId::new();
        let participant = ParticipantData {
            profile: MediaProfile {
                id: profile_id,
                family_id: agent_storage::FamilyId::new(),
                name: "Viewer".into(),
                user_id: None,
                active: true,
            },
            preferences: ProfilePreferences::new(profile_id),
            restrictions: ContentRestrictions::new(profile_id),
            watched_items: HashSet::new(),
            history_complete: false,
            genre_affinity: HashMap::new(),
        };
        let (watched_by, unknown) = watched_evidence(item().id, &[participant]);
        assert!(watched_by.is_empty());
        assert_eq!(unknown, vec![profile_id]);
        assert!(!should_exclude_watched(WatchedPolicy::AnyParticipant, 1, 0));
    }

    #[test]
    fn shared_preferences_and_selected_participant_preferences_contribute_to_score() {
        let mut family = FamilyMediaPreferences::new(agent_storage::FamilyId::new());
        family.liked_genres.push("Comedy".into());
        let profile = MediaProfile {
            id: MediaProfileId::new(),
            family_id: family.family_id,
            name: "Selected".into(),
            user_id: None,
            active: true,
        };
        let mut preferences = ProfilePreferences::new(profile.id);
        preferences.liked_genres.push("Comedy".into());
        let participant = ParticipantData {
            profile,
            preferences,
            restrictions: ContentRestrictions::new(MediaProfileId::new()),
            watched_items: HashSet::new(),
            history_complete: true,
            genre_affinity: HashMap::new(),
        };

        let (score, reasons) = score_item(&item(), &family, &[participant]);
        assert!(score > 600.0);
        assert!(
            reasons
                .iter()
                .any(|reason| reason.contains("shared family"))
        );
        assert!(reasons.iter().any(|reason| reason.contains("Selected")));
    }
}
