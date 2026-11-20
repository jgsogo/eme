-- Durable run snapshots and lifecycle events. Conversation messages are
-- extended by SqliteExecutionStore using an idempotent column migration.
CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT REFERENCES conversations(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    run_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runs_conversation_created
    ON runs(conversation_id, created_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS run_events (
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK(sequence >= 0),
    occurred_at TEXT NOT NULL,
    event_json TEXT NOT NULL,
    PRIMARY KEY(run_id, sequence)
);
