CREATE TABLE IF NOT EXISTS media_external_connections (
    profile_id TEXT NOT NULL REFERENCES media_profiles(id) ON DELETE CASCADE,
    family_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    external_connection_id TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    provider_subject TEXT NOT NULL,
    display_name TEXT,
    linked_at TEXT NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY(profile_id, provider)
);
CREATE INDEX IF NOT EXISTS idx_media_external_connections_family
    ON media_external_connections(family_id, provider);

CREATE TABLE IF NOT EXISTS pending_media_connections (
    state_hash TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    payload TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);
