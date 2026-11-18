CREATE TABLE IF NOT EXISTS external_connections (
 id TEXT PRIMARY KEY NOT NULL,
 owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 provider TEXT NOT NULL CHECK(length(trim(provider)) > 0),
 provider_subject TEXT NOT NULL CHECK(length(trim(provider_subject)) > 0),
 display_email TEXT,
 encrypted_refresh_token BLOB NOT NULL,
 granted_scopes TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('active','revoked')),
 created_at TEXT NOT NULL,
 updated_at TEXT NOT NULL,
 revoked_at TEXT,
 UNIQUE(owner_id,provider,provider_subject),
 CHECK((status='revoked')=(revoked_at IS NOT NULL))
);
CREATE INDEX IF NOT EXISTS external_connections_owner_idx ON external_connections(owner_id,provider,status);
CREATE TABLE IF NOT EXISTS pending_external_authorizations (
 id TEXT PRIMARY KEY NOT NULL,
 user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 provider TEXT NOT NULL CHECK(length(trim(provider)) > 0),
 state_hash TEXT NOT NULL UNIQUE CHECK(length(state_hash)=64),
 encrypted_pkce_verifier BLOB NOT NULL,
 requested_scopes TEXT NOT NULL,
 created_at TEXT NOT NULL,
 expires_at TEXT NOT NULL,
 consumed_at TEXT,
 CHECK(expires_at > created_at)
);
CREATE INDEX IF NOT EXISTS pending_external_authorizations_expiry_idx ON pending_external_authorizations(expires_at);
