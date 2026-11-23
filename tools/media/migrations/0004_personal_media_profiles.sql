-- Personal media profiles are owned directly by an EME user and do not
-- require a family membership. Preserve all existing family profile data.
PRAGMA foreign_keys = OFF;

CREATE TABLE media_profiles_new (
    id TEXT PRIMARY KEY NOT NULL,
    family_id TEXT,
    owner_id TEXT,
    user_id TEXT,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK ((family_id IS NOT NULL AND owner_id IS NULL) OR
           (family_id IS NULL AND owner_id IS NOT NULL))
);
INSERT INTO media_profiles_new(id,family_id,owner_id,user_id,name,active,payload,created_at,updated_at)
SELECT id,family_id,NULL,user_id,name,active,payload,created_at,updated_at
FROM media_profiles;
DROP TABLE media_profiles;
ALTER TABLE media_profiles_new RENAME TO media_profiles;

CREATE INDEX idx_media_profiles_family
    ON media_profiles(family_id, active, name);
CREATE UNIQUE INDEX idx_media_profiles_personal_owner
    ON media_profiles(owner_id) WHERE owner_id IS NOT NULL;

CREATE TABLE media_external_connections_new (
    profile_id TEXT NOT NULL REFERENCES media_profiles(id) ON DELETE CASCADE,
    family_id TEXT,
    provider TEXT NOT NULL,
    external_connection_id TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    provider_subject TEXT NOT NULL,
    display_name TEXT,
    linked_at TEXT NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY(profile_id, provider)
);
INSERT INTO media_external_connections_new(
    profile_id,family_id,provider,external_connection_id,owner_id,
    provider_subject,display_name,linked_at,payload
)
SELECT profile_id,family_id,provider,external_connection_id,owner_id,
       provider_subject,display_name,linked_at,payload
FROM media_external_connections;
DROP TABLE media_external_connections;
ALTER TABLE media_external_connections_new RENAME TO media_external_connections;
CREATE INDEX idx_media_external_connections_family
    ON media_external_connections(family_id, provider);

PRAGMA foreign_keys = ON;
