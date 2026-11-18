DROP TABLE recipes;

CREATE TABLE recipes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    owner_id TEXT NOT NULL,
    family_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX recipes_title ON recipes(lower(title));
CREATE INDEX recipes_owner_updated ON recipes(owner_id, updated_at DESC);
CREATE INDEX recipes_family_updated ON recipes(family_id, updated_at DESC);
