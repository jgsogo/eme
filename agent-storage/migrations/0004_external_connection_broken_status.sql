-- Allow a provider connection to be marked broken after a permanent
-- authentication/authorization failure. Preserve existing connection rows.
PRAGMA foreign_keys=OFF;
BEGIN IMMEDIATE;

CREATE TABLE external_connections_new (
 id TEXT PRIMARY KEY NOT NULL,
 owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 provider TEXT NOT NULL CHECK(length(trim(provider)) > 0),
 provider_subject TEXT NOT NULL CHECK(length(trim(provider_subject)) > 0),
 display_email TEXT,
 encrypted_refresh_token BLOB NOT NULL,
 granted_scopes TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('active','broken','revoked')),
 created_at TEXT NOT NULL,
 updated_at TEXT NOT NULL,
 revoked_at TEXT,
 UNIQUE(owner_id,provider,provider_subject),
 CHECK((status='revoked')=(revoked_at IS NOT NULL))
);

INSERT INTO external_connections_new (
 id, owner_id, provider, provider_subject, display_email,
 encrypted_refresh_token, granted_scopes, status, created_at, updated_at, revoked_at
)
SELECT
 id, owner_id, provider, provider_subject, display_email,
 encrypted_refresh_token, granted_scopes, status, created_at, updated_at, revoked_at
FROM external_connections;

DROP TABLE external_connections;
ALTER TABLE external_connections_new RENAME TO external_connections;

CREATE INDEX IF NOT EXISTS external_connections_owner_idx
 ON external_connections(owner_id,provider,status);

COMMIT;
PRAGMA foreign_keys=ON;
