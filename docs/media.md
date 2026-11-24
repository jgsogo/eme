# Media discovery and family recommendations

The media feature provides movie and TV discovery, family-aware recommendations, Spanish streaming availability, and optional Trakt viewing-history synchronization.

It is implemented in the single Rust crate `tools/media` (`media-tools`). Provider DTOs are normalized into EME domain types before they reach storage, recommendation logic, or agent tools.

## Providers

| Provider | Responsibility |
|---|---|
| [TMDB](https://developer.themoviedb.org/docs) | Movie/TV search, popular discovery, normalized metadata, genres, runtime, audience ratings and regional certification |
| [Watchmode](https://api.watchmode.com/docs/) | Primary availability source: resolve TMDB IDs to Watchmode titles and retrieve regional streaming offers |
| TMDB watch providers | Fallback availability source when Watchmode has no usable offers |
| [Trakt](https://trakt.docs.apiary.io/) | Optional OAuth account linking, bidirectional ratings, watched-history writes, watchlist writes and import of watched movies/shows, ratings and watchlists |

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
family_theme_preference = 60
family_theme_dislike_penalty = 60
family_language_preference = 40
profile_genre_preference = 60
profile_genre_dislike_penalty = 150
profile_theme_preference = 40
profile_theme_dislike_penalty = 100
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

TMDB and Watchmode tools are registered even when their credentials are absent, so the server can start in a partially configured environment. Calls that require an unconfigured provider return a clear configuration error. Recommendations require TMDB. Watchmode is preferred for availability; TMDB's regional watch-provider endpoint is the fallback when Watchmode is absent, errors or has no matching configured subscription. Trakt linking and synchronization are enabled only when all three Trakt-related variables are configured. Agent writes use the same authenticated Trakt OAuth connection as private history imports; no separate scope migration is required.

## Personal and family profiles

EME identity and family membership remain authoritative in `agent-storage`. Every authenticated EME user gets one personal media profile, created automatically and idempotently the first time the media integration is accessed. A personal profile is owned directly by its EME `UserId` and has no family association. Users do not need to create or join a family to connect Trakt.

Family-managed profiles remain separate records. Family owners/admins can create, update, deactivate and delete them, and optionally assign an existing family user through the profile `user_id` field.

- A personal profile is accessible only to its owning EME user.
- Each signed-in family member can still link a Trakt account to a family-managed profile whose `user_id` is their own EME `UserId`. Family administration rights do not allow one member to attach their Trakt credentials to another member's profile.
- A personal profile's owner, or a family profile's associated user / family owner / admin, can manage its explicit preferences and content restrictions.
- Family owners/admins manage shared family preferences.
- The server derives ownership and family membership from authenticated context and stored records. A model-supplied family ID is never an authorization boundary.
- A recommendation request must identify its participants. The assistant should ask who is watching rather than silently using every family profile.

The `media_profiles` tool continues to manage family profiles. Personal profiles are provisioned by the authenticated application path. A profile with a Trakt connection must be unlinked before it can be deleted.

## Preferences, restrictions and observed activity

These are deliberately separate:

1. **Shared family preferences** always contribute to family recommendations.
2. **Individual explicit preferences** contribute only for selected participants.
3. **Content restrictions** are hard suitability constraints and are applied to every selected participant before ranking.
4. **Trakt history and ratings** are observed activity with source provenance; they do not overwrite explicit preferences.
5. **Inferred genre affinity** is calculated dynamically from selected participants' ratings and catalogue metadata. The inferred result is not persisted as an explicit preference.

Ordinary dislikes are ranking penalties, not hard exclusions. A selected participant's explicit genre dislike suppresses positive profile-preference and inferred-affinity bonuses for that same genre from other selected participants, so one person's strong negative is not cancelled by another person's positive. Unknown history is not treated as a confirmed unwatched state.

### Watched-title policy

The global default is `any_participant`; a recommendation request may override it. Family preference weights and selected-profile weights (including genre and theme signals) are independently configurable under `[recommendation_weights]`. The listed values are starting defaults, not fixed product policy; set any weight to zero to disable that signal. Theme matching currently uses normalized title and overview text.

- `any_participant`: exclude a title if any selected participant is known to have watched it.
- `all_participants`: exclude only if every selected participant is known to have watched it.
- `include_all`: do not exclude watched titles.

Incomplete history is returned as uncertainty evidence. It is not silently interpreted as “not watched”.

## Trakt account linking and synchronization

Trakt accounts are linked to individual media profiles, not to the family as one undifferentiated account. Every EME user can link Trakt to their automatically provisioned personal profile without belonging to a family. Family members may additionally link a family-managed profile assigned to them (`MediaProfile.user_id`). OAuth uses PKCE and one-time state bound to the initiating EME user and profile. Refresh tokens are stored through the existing encrypted external-connection store and rotated tokens replace the previous credentials.

Authenticated API endpoints:

- `GET /media/trakt/profiles` — list the signed-in user's personal profile, assigned family profiles, and their Trakt connection/sync status.
- `POST /media/trakt/link` — JSON body: `{"profile_id":"…"}` for a personal profile; include `family_id` only for a family-managed profile.
- `GET /media/trakt/callback?code=…&state=…` — OAuth callback. The callback must arrive in the initiating user's authenticated browser session.
- `POST /media/trakt/profiles/{profile_id}/sync` — synchronize a personal profile; add `?family_id=…` only for a family-managed profile.
- `DELETE /media/trakt/profiles/{profile_id}` — empty JSON body for a personal profile; include `family_id` only for a family-managed profile.

The Profile → Integrations view always lists the signed-in user's personal media profile and any assigned family profiles, with Connect, Sync and Disconnect actions. No family membership or family-admin action is required to connect a personal Trakt account. The initial and incremental sync imports watched movies/shows, ratings and watchlist entries. Activity timestamps determine which categories need refreshing. Sync is explicit; recommendations do not trigger a Trakt sync. Agent watchlist mutations, watched-history events and EME ratings are written through immediately when the profile's Trakt connection belongs to the authenticated user.

### Ownership and write semantics

- Every watchlist, watched-history and rating mutation is scoped to one media profile and must follow an explicit user request; the agent must never infer a rating, mark a title watched or mutate a watchlist proactively. The profile's stored Trakt connection is the only account the agent can mutate; the model cannot supply a Trakt username, account ID or credential.
- A watchlist `add` or `remove` requires a linked Trakt account owned by the authenticated user and a TMDB ID plus media kind. EME sends the mutation directly to Trakt and verifies that Trakt resolved the title. Linked profiles do not maintain a separately writable local watchlist mirror. Repeating an add or remove is safe. Trakt account limits and rate limits are respected; provider errors are returned rather than bypassed.
- Watchlist `list` reads the current movie and show watchlists directly from Trakt for linked profiles; it does not depend on the synchronization cursor or local mirror. If no Trakt account is linked, it reads the local watchlist. Trakt errors are returned rather than falling back to potentially stale local data.
- `media_watch_history` defaults to `action=list`; `action=mark_watched` requires `tmdb_id`, `kind` and the caller-owned Trakt connection. EME writes to Trakt's `/sync/history` endpoint first and records the local Trakt-provenance event only after the provider confirms the item. It does not silently fall back to local-only history writes.\n- `media_rate` always records an EME-originated rating. If the selected profile has no Trakt link, the rating is stored locally. If it has a Trakt link owned by the caller, EME writes the rating to Trakt before persisting the local EME record. If the linked account belongs to another user, the write is rejected; family-admin access does not grant permission to act as another user's Trakt account.
- EME ratings use 0–5; Trakt accepts integer ratings from 1–10. Conversion is linear with rounding: EME `0` maps to Trakt `1`, EME `5` maps to Trakt `10`; intermediate values are quantized. Imported Trakt ratings use the inverse mapping, `(trakt_rating - 1) * 5 / 9`.
- EME and Trakt rating records retain separate source provenance. When both exist for a title, recommendation genre affinity uses the newest `rated_at` record. EME wins exact timestamp ties to preserve its finer-grained value after write-through; a later Trakt rating supersedes it without deleting either record.

No playback scraping or automatic scrobbling is performed.

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

The availability tool returns all supported offer types (subscription, rent, buy, free and ad-supported). Recommendations require a configured subscription by default and therefore retain only matching subscription offers. If Watchmode has no usable offer, EME falls back to TMDB's regional watch-provider response. The `media_watch_history` tool accepts `action=list|mark_watched`; marking a title watched requires `tmdb_id` and `kind` and writes through to the caller-owned Trakt connection. The `media_watchlist` tool accepts `action=list|add|remove`; add/remove require `tmdb_id` and `kind`. The `media_rate` tool accepts an optional `profile_id` and writes through to the caller-owned Trakt connection when one is linked.

Recommendation output includes normalized catalogue metadata, score, reasons, matching subscription offers, known watched participants and profiles with incomplete history.

## Storage and observability

Media data is stored in `data/media/media.sqlite`. Encrypted provider credentials remain in the shared `data/external_connections.sqlite` store. Family membership is not copied into the media database.

Tracing records provider request status and latency, recommendation candidate/eligibility/return counts, exclusion counts by criteria/restriction/watched/availability, history uncertainty counts and Trakt sync category counts. Access tokens, refresh tokens, API keys and authorization codes are never included in tracing fields.

## Reproducible smoke test

The mocked provider pipeline can be exercised without API keys or external accounts:

```sh
cargo test -p media-tools recommendation_e2e_uses_only_selected_profiles_and_requires_subscription_offer
cargo test -p media-tools recommendation_tool_executes_the_provider_pipeline_for_selected_profiles
cargo test -p media-tools falls_back_to_tmdb_when_watchmode_has_no_title_match
```

For a live assistant smoke test, create a managed profile with `media_profiles` (`action = "create"`), then pass the returned profile ID as the sole entry in `media_recommend.participant_ids`. The tool will use the configured Spanish catalogue and subscription set.

## Current boundaries

- Catalogue region defaults to Spain; metadata language defaults to `es-ES`, while original-language metadata is retained.
- Watchmode provider/source IDs are resolved from API responses, not hard-coded.
- Trakt watched episodes are represented at the TV-show level for watched-title exclusion.
- Agent watchlist add/remove, mark-watched and EME rating writes are supported for a profile's caller-owned Trakt connection; local records are updated only after successful provider writes.
- No direct streaming-platform authentication, playback, scraping, ML/embedding ranking or proactive release notifications are included.
