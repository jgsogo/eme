-- Family invitations store only SHA-256 hashes of bearer tokens.
CREATE TABLE IF NOT EXISTS family_invitations (
    id TEXT PRIMARY KEY NOT NULL,
    family_id TEXT NOT NULL REFERENCES families(id) ON DELETE CASCADE,
    inviter_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    target_provider TEXT,
    target_subject TEXT,
    token_hash TEXT NOT NULL UNIQUE CHECK (length(token_hash) = 64),
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    accepted_at TEXT,
    accepted_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    revoked_at TEXT,
    CHECK ((target_provider IS NULL) = (target_subject IS NULL)),
    CHECK (target_provider IS NULL OR length(trim(target_provider)) > 0),
    CHECK (target_subject IS NULL OR length(trim(target_subject)) > 0),
    CHECK (expires_at > created_at),
    CHECK (accepted_at IS NULL OR revoked_at IS NULL)
);
CREATE INDEX IF NOT EXISTS family_invitations_family_id_idx
    ON family_invitations(family_id, created_at);
