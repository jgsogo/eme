PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS media_profiles (
    id TEXT PRIMARY KEY NOT NULL,
    family_id TEXT NOT NULL,
    user_id TEXT,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_media_profiles_family
    ON media_profiles(family_id, active, name);

CREATE TABLE IF NOT EXISTS profile_preferences (
    profile_id TEXT PRIMARY KEY NOT NULL
        REFERENCES media_profiles(id) ON DELETE CASCADE,
    payload TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS profile_restrictions (
    profile_id TEXT PRIMARY KEY NOT NULL
        REFERENCES media_profiles(id) ON DELETE CASCADE,
    payload TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS family_preferences (
    family_id TEXT PRIMARY KEY NOT NULL,
    payload TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS media_items (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    payload TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_media_items_kind ON media_items(kind);

CREATE TABLE IF NOT EXISTS watch_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id TEXT NOT NULL
        REFERENCES media_profiles(id) ON DELETE CASCADE,
    media_item_id TEXT NOT NULL
        REFERENCES media_items(id) ON DELETE CASCADE,
    watched_at TEXT NOT NULL,
    source TEXT NOT NULL,
    payload TEXT NOT NULL,
    UNIQUE(profile_id, media_item_id, watched_at, source)
);
CREATE INDEX IF NOT EXISTS idx_watch_history_profile
    ON watch_history(profile_id, watched_at DESC);

CREATE TABLE IF NOT EXISTS media_ratings (
    profile_id TEXT NOT NULL
        REFERENCES media_profiles(id) ON DELETE CASCADE,
    media_item_id TEXT NOT NULL
        REFERENCES media_items(id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    value REAL NOT NULL CHECK (value >= 0.0 AND value <= 5.0),
    rated_at TEXT NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY(profile_id, media_item_id, source)
);
CREATE INDEX IF NOT EXISTS idx_media_ratings_profile
    ON media_ratings(profile_id, rated_at DESC);

CREATE TABLE IF NOT EXISTS streaming_offers (
    media_item_id TEXT NOT NULL
        REFERENCES media_items(id) ON DELETE CASCADE,
    service_id TEXT NOT NULL,
    offer_type TEXT NOT NULL,
    region TEXT NOT NULL,
    payload TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(media_item_id, service_id, offer_type, region)
);
CREATE INDEX IF NOT EXISTS idx_streaming_offers_region
    ON streaming_offers(region, service_id);

CREATE TABLE IF NOT EXISTS provider_sync_state (
    profile_id TEXT NOT NULL
        REFERENCES media_profiles(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    cursor TEXT,
    last_synced_at TEXT,
    status TEXT NOT NULL DEFAULT 'idle',
    PRIMARY KEY(profile_id, provider)
);
