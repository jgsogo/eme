CREATE TABLE IF NOT EXISTS connected_calendars (
    id TEXT PRIMARY KEY NOT NULL,
    owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    connection_id TEXT NOT NULL REFERENCES external_connections(id) ON DELETE CASCADE,
    provider_calendar_id TEXT NOT NULL CHECK(length(trim(provider_calendar_id)) > 0),
    display_name TEXT,
    time_zone TEXT,
    provider_access_role TEXT,
    selected INTEGER NOT NULL DEFAULT 0 CHECK(selected IN (0,1)),
    access_mode TEXT NOT NULL DEFAULT 'read_only' CHECK(access_mode IN ('read_only','read_write')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(connection_id, provider_calendar_id)
);
CREATE INDEX IF NOT EXISTS connected_calendars_owner_idx
    ON connected_calendars(owner_id, connection_id, selected);
