ALTER TABLE recipes ADD COLUMN owner_id TEXT NOT NULL DEFAULT '';
ALTER TABLE recipes ADD COLUMN family_id TEXT;
CREATE INDEX recipes_owner_updated ON recipes(owner_id,updated_at DESC);
CREATE INDEX recipes_family_updated ON recipes(family_id,updated_at DESC);
