# Media discovery and family recommendations

The media feature provides movie and TV discovery, family-aware recommendations, Spanish streaming availability, and optional Trakt viewing-history synchronization.

It is implemented in the single Rust crate `tools/media` (`media-tools`). Provider DTOs are normalized into EME domain types before they reach storage, recommendation logic, or agent tools.

## Providers

| Provider | Responsibility |
|---|---|
| [TMDB](https://developer.themoviedb.org/docs) | Movie/TV search, popular discovery, normalized metadata, genres, runtime, audience ratings and regional certification |
| [Watchmode](https://api.watchmode.com/docs/) | Primary availability source: resolve TMDB IDs to Watchmode titles and retrieve regional streaming offers |
| TMDB watch providers | Fallback availability source when Watchmode has no usable offers |
| [Trakt](https://trakt.docs.apiary.io/) | Optional OAuth account linking and import of watched movies/shows, ratings and watchlists |

EME does not log in to Netflix, Prime Video, Disney+ or Max. Household subscriptions are configuration used to filter Watchmode offers; they are not streaming-service credentials.

Keep provider names, offer types and provider URLs intact when presenting results. Before production rollout, verify the current attribution, caching and rate-limit requirements for the TMDB and Watchmode API plans in use; this implementation uses a configurable six-hour Watchmode cache and respects provider-supplied retry information.

## Configuration

The server reads `data/media/config.toml`. If the file is absent, the Rust defaults are used.

```toml
country = "ES"
language = "es-ES"
subscriptions = ["netflix", "prime_video", "disney_plus", "hbo_max"]
recommendation_limit = 5
require_subscription = true
availability_cache_ttl_seconds = 21600
watched_policy = "any_participant"

[recommendation_weights]
audience_rating_scale = 100
family_genre_preference = 100
family_genre_dislike_penalty = 100
family_language_preference = 40
profile_genre_preference = 60
profile_genre_dislike_penalty = 150
profile_language_preference = 30
rating_affinity_scale = 30
audience_quality_bonus = 40
popularity_scale = 10
recent_release_bonus = 30
older_release_bonus = 15
recent_release_years = 2
older_release_years = 5
diversity_penalty = 25
```

Provider credentials are environment variables, never TOML values:

| Variable | Purpose |
|---|---|
| `TMDB_API_KEY` | Enables catalogue search, details and discovery |
| `WATCHMODE_API_KEY` | Enables regional availability and subscription-filtered recommendations |
| `TRAKT_CLIENT_ID` | Trakt OAuth client ID |
| `TRAKT_REDIRECT_URI` | Must match the registered callback URL, normally `https://<host>/media/trakt/callback` |
| `EME_CREDENTIAL_ENCRYPTION_KEY` | Existing 32-byte key encoded as 64 hexadecimal characters; encrypts external credentials |

TMDB and Watchmode tools are registered even when their credentials are absent, so the server can start in a partially configured environment. Calls that require an unconfigured provider return a clear configuration error. Recommendations require TMDB. Watchmode is preferred for availability; TMDB's regional watch-provider endpoint is the fallback when Watchmode is absent, errors or has no matching configured subscription. Trakt linking and synchronization are enabled only when all three Trakt-related variables are configured.

## Family profiles and permissions

EME family membership remains authoritative in `agent-storage`. Media profiles are separate, family-managed records and may exist without an EME login. A profile can optionally be associated with an existing family user's `UserId`.

- Family owners/admins can create, update, deactivate and delete media profiles.
- A profile's associated user or a family owner/admin can update that profile's explicit preferences and content restrictions.
- Family owners/admins manage shared family preferences.
- The server derives the family from the authenticated tool context. A model-supplied family ID is never an authorization boundary.
- A recommendation request must identify its participants. The assistant should ask who is watching rather than silently using every family profile.

The `media_profiles` tool lists profiles and supports `create`, `update` and `delete` actions. A profile with a Trakt connection must be unlinked before it can be deleted.

## Preferences, restrictions and observed activity

These are deliberately separate:

1. **Shared family preferences** always contribute to family recommendations.
2. **Individual explicit preferences** contribute only for selected participants.
3. **Content restrictions** are hard suitability constraints and are applied to every selected participant before ranking.
4. **Trakt history and ratings** are observed activity with source provenance; they do not overwrite explicit preferences.
5. **Inferred genre affinity** is calculated dynamically from selected participants' ratings and catalogue metadata. The inferred result is not persisted as an explicit preference.

Ordinary dislikes are ranking penalties, not hard exclusions. A selected participant's explicit genre dislike suppresses positive profile-preference and inferred-affinity bonuses for that same genre from other selected participants, so one person's strong negative is not cancelled by another person's positive. Unknown history is not treated as a confirmed unwatched state.

### Watched-title policy

The global default is `any_participant`; a recommendation request may override it. Family preference weights and selected-profile weights are independently configurable under `[recommendation_weights]`. The listed values are starting defaults, not fixed product policy; set any weight to zero to disable that signal.

- `any_participant`: exclude a title if any selected participant is known to have watched it.
- `all_participants`: exclude only if every selected participant is known to have watched it.
- `include_all`: do not exclude watched titles.

Incomplete history is returned as uncertainty evidence. It is not silently interpreted as “not watched”.

## Trakt account linking and synchronization

Trakt accounts are linked to individual media profiles, not to the family as one undifferentiated account. OAuth uses PKCE and one-time state bound to the initiating EME user. Refresh tokens are stored through the existing encrypted external-connection store and rotated tokens replace the previous credentials.

Authenticated API endpoints:

- `POST /media/trakt/link` — JSON body: `{"family_id":"…","profile_id":"…"}`; returns the authorization URL.
- `GET /media/trakt/callback?code=…&state=…` — OAuth callback. The callback must arrive in the initiating user's authenticated browser session.
- `POST /media/trakt/profiles/{profile_id}/sync?family_id=…` — explicitly synchronize changed Trakt activity categories.
- `DELETE /media/trakt/profiles/{profile_id}` — JSON body: `{"family_id":"…"}`; unlink and revoke the stored connection.

The initial and incremental sync imports watched movies/shows, ratings and watchlist entries. Activity timestamps determine which categories need refreshing. Sync is explicit; recommendations do not trigger a Trakt sync. No playback scraping or automatic scrobbling is performed.

## Agent tools

The configured assistant registers:

- `media_profiles`
- `media_recommend`
- `media_search`
- `media_details`
- `media_availability`
- `media_watch_history`
- `media_watchlist`
- `media_rate`
- `media_preferences`
- `media_sync_history`

The availability tool returns all supported offer types (subscription, rent, buy, free and ad-supported). Recommendations require a configured subscription by default and therefore retain only matching subscription offers. If Watchmode has no usable offer, EME falls back to TMDB's regional watch-provider response.

Recommendation output includes normalized catalogue metadata, score, reasons, matching subscription offers, known watched participants and profiles with incomplete history.

## Storage and observability

Media data is stored in `data/media/media.sqlite`. Encrypted provider credentials remain in the shared `data/external_connections.sqlite` store. Family membership is not copied into the media database.

Tracing records provider request status and latency, recommendation candidate/eligibility/return counts, exclusion counts by criteria/restriction/watched/availability, history uncertainty counts and Trakt sync category counts. Access tokens, refresh tokens, API keys and authorization codes are never included in tracing fields.

## Current boundaries

- Catalogue region defaults to Spain; metadata language defaults to `es-ES`, while original-language metadata is retained.
- Watchmode provider/source IDs are resolved from API responses, not hard-coded.
- Trakt watched episodes are represented at the TV-show level for watched-title exclusion.
- Watchlist tools currently expose synchronized entries for reading; they do not write to Trakt.
- No direct streaming-platform authentication, playback, scraping, ML/embedding ranking or proactive release notifications are included.
