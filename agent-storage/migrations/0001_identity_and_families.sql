-- Initial identity and family schema.
-- Apply with SQLite foreign_keys=ON. Family deletion cascades to memberships;
-- user deletion is restricted while the user is the historical family creator.
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS user_identities (
    provider TEXT NOT NULL CHECK (length(trim(provider)) > 0),
    subject TEXT NOT NULL CHECK (length(trim(subject)) > 0),
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY (provider, subject)
);
CREATE INDEX IF NOT EXISTS user_identities_user_id_idx ON user_identities(user_id);

CREATE TABLE IF NOT EXISTS families (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    creator_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS family_members (
    family_id TEXT NOT NULL REFERENCES families(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member')),
    relationship TEXT NOT NULL CHECK (relationship IN ('father', 'mother', 'son', 'daughter', 'grandfather', 'grandmother', 'partner', 'other')),
    joined_at TEXT NOT NULL,
    PRIMARY KEY (family_id, user_id)
);
CREATE UNIQUE INDEX IF NOT EXISTS family_members_one_owner_idx
    ON family_members(family_id) WHERE role = 'owner';
CREATE INDEX IF NOT EXISTS family_members_user_id_idx ON family_members(user_id);

-- The family service must create the family and its owner membership in one
-- transaction; the partial index enforces at most one owner at storage level.
