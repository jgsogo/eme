-- Canonical conversation metadata and ordered model-visible messages.
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY NOT NULL,
    owner TEXT NOT NULL,
    agent TEXT NOT NULL,
    title TEXT NOT NULL,
    selected_family_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_conversations_owner_activity
    ON conversations(owner, updated_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS conversation_messages (
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK(sequence >= 0),
    message_json TEXT NOT NULL,
    PRIMARY KEY(conversation_id, sequence)
);

CREATE TABLE IF NOT EXISTS active_conversations (
    owner TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE
);
