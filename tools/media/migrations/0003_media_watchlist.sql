CREATE TABLE IF NOT EXISTS media_watchlist (
    profile_id TEXT NOT NULL REFERENCES media_profiles(id) ON DELETE CASCADE,
    media_item_id TEXT NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    added_at TEXT NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY(profile_id, media_item_id, source)
);
CREATE INDEX IF NOT EXISTS idx_media_watchlist_profile
    ON media_watchlist(profile_id, added_at DESC);
