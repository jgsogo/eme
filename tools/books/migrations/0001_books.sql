PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS user_books (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL CHECK (length(trim(user_id)) > 0),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    authors_json TEXT NOT NULL DEFAULT '[]',
    isbn TEXT,
    publisher TEXT,
    publication_year INTEGER,
    catalogue_url TEXT,
    rating INTEGER CHECK (rating IS NULL OR rating BETWEEN 1 AND 5),
    reading_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (reading_status IN ('pending', 'reading', 'read')),
    want_to_buy INTEGER NOT NULL DEFAULT 0 CHECK (want_to_buy IN (0, 1)),
    review TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS user_books_owner_updated_idx
    ON user_books(user_id, updated_at DESC, id);

CREATE TABLE IF NOT EXISTS user_library_preferences (
    user_id TEXT NOT NULL CHECK (length(trim(user_id)) > 0),
    library_id TEXT NOT NULL CHECK (length(trim(library_id)) > 0),
    library_name TEXT NOT NULL CHECK (length(trim(library_name)) > 0),
    PRIMARY KEY (user_id, library_id)
);
