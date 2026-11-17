CREATE TABLE IF NOT EXISTS notes (
    id TEXT PRIMARY KEY NOT NULL,
    owner_id TEXT NOT NULL,
    family_id TEXT,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS notes_owner_updated_idx ON notes(owner_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS notes_family_updated_idx ON notes(family_id, updated_at DESC);
