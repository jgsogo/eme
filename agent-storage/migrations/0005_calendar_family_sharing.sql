-- Family sharing is an EME-level delegation. The family database is a
-- separate SQLite database, so family membership is validated by FamilyService
-- and rechecked by the runtime authorization layer rather than by a FK here.
ALTER TABLE connected_calendars ADD COLUMN family_id TEXT;
ALTER TABLE connected_calendars ADD COLUMN family_access TEXT
    CHECK(family_access IS NULL OR family_access IN ('viewer','contributor'));

CREATE TRIGGER IF NOT EXISTS connected_calendars_family_share_insert
BEFORE INSERT ON connected_calendars
WHEN (NEW.family_id IS NULL) != (NEW.family_access IS NULL)
BEGIN
    SELECT RAISE(ABORT, 'family_id and family_access must be set together');
END;

CREATE TRIGGER IF NOT EXISTS connected_calendars_family_share_update
BEFORE UPDATE OF family_id, family_access ON connected_calendars
WHEN (NEW.family_id IS NULL) != (NEW.family_access IS NULL)
BEGIN
    SELECT RAISE(ABORT, 'family_id and family_access must be set together');
END;
