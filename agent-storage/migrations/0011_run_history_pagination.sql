-- Support newest-first run history pagination without scanning the full table.
CREATE INDEX IF NOT EXISTS idx_runs_created_at_id
    ON runs(created_at DESC, id DESC);
