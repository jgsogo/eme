-- Persist the authenticated owner of one-shot executions independently of
-- their temporary conversation, which is deleted when execution finishes.
CREATE TABLE IF NOT EXISTS one_shot_run_owners (
    run_id TEXT PRIMARY KEY NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    owner TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_one_shot_run_owners_owner
    ON one_shot_run_owners(owner);
