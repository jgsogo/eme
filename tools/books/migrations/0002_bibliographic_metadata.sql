ALTER TABLE user_books ADD COLUMN responsibility TEXT;
ALTER TABLE user_books ADD COLUMN publication_statement TEXT;
ALTER TABLE user_books ADD COLUMN physical_description TEXT;
ALTER TABLE user_books ADD COLUMN series TEXT;
ALTER TABLE user_books ADD COLUMN notes_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE user_books ADD COLUMN subjects_json TEXT NOT NULL DEFAULT '[]';
