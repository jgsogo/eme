CREATE TABLE family_members (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 name TEXT NOT NULL CHECK (length(trim(name)) > 0),
 active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1))
);
CREATE TABLE recurring_participation_rules (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 member_id INTEGER NOT NULL REFERENCES family_members(id) ON DELETE RESTRICT,
 weekday INTEGER NOT NULL CHECK (weekday BETWEEN 0 AND 6),
 meal_type TEXT NOT NULL CHECK (meal_type IN ('breakfast', 'lunch', 'dinner')),
 arrangement TEXT NOT NULL CHECK (arrangement IN ('home', 'school', 'away', 'not_participating')),
 effective_from TEXT,
 effective_until TEXT,
 CHECK (effective_from IS NULL OR effective_until IS NULL OR effective_from <= effective_until)
);
CREATE INDEX recurring_rules_lookup ON recurring_participation_rules(member_id, weekday, meal_type);
CREATE TABLE meal_participation_exceptions (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 member_id INTEGER NOT NULL REFERENCES family_members(id) ON DELETE RESTRICT,
 date TEXT NOT NULL,
 meal_type TEXT NOT NULL CHECK (meal_type IN ('breakfast', 'lunch', 'dinner')),
 arrangement TEXT NOT NULL CHECK (arrangement IN ('home', 'school', 'away', 'not_participating')),
 UNIQUE(member_id, date, meal_type)
);
CREATE INDEX meal_exceptions_date ON meal_participation_exceptions(date, meal_type);
