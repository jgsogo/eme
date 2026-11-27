-- Soft deletion preserves conversation metadata and message history for reporting.
ALTER TABLE conversations ADD COLUMN removed_at TEXT;

CREATE INDEX IF NOT EXISTS idx_conversations_owner_removed_activity
    ON conversations(owner, removed_at, updated_at DESC, id DESC);
