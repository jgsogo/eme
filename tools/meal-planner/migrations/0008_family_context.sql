-- Family-scoped Meals data. The previous local household model is replaced
-- by a cache of the authoritative family membership records.
DROP TABLE IF EXISTS shopping_lists;
DROP TABLE IF EXISTS weekly_plans;
DROP TABLE IF EXISTS meal_participation_exceptions;
DROP TABLE IF EXISTS recurring_participation_rules;
DROP TABLE IF EXISTS family_members;
DROP TABLE IF EXISTS school_menus;

CREATE TABLE family_members (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    family_id TEXT NOT NULL DEFAULT 'legacy',
    external_id TEXT NOT NULL DEFAULT '',
    name TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 1,
    UNIQUE(family_id, external_id)
);
CREATE INDEX idx_family_members_family ON family_members(family_id, active);

CREATE TABLE recurring_participation_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    family_id TEXT NOT NULL,
    member_id INTEGER NOT NULL REFERENCES family_members(id) ON DELETE RESTRICT,
    weekday INTEGER NOT NULL CHECK(weekday BETWEEN 0 AND 6),
    meal_type TEXT NOT NULL,
    arrangement TEXT NOT NULL,
    effective_from TEXT,
    effective_until TEXT
);
CREATE INDEX idx_participation_rules_family ON recurring_participation_rules(family_id, member_id);

CREATE TABLE meal_participation_exceptions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    family_id TEXT NOT NULL,
    member_id INTEGER NOT NULL REFERENCES family_members(id) ON DELETE RESTRICT,
    date TEXT NOT NULL,
    meal_type TEXT NOT NULL,
    arrangement TEXT NOT NULL,
    UNIQUE(family_id, member_id, date, meal_type)
);
CREATE INDEX idx_participation_exceptions_family ON meal_participation_exceptions(family_id, date);

CREATE TABLE weekly_plans (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    family_id TEXT NOT NULL,
    week_start TEXT NOT NULL,
    revision INTEGER NOT NULL,
    status TEXT NOT NULL,
    assignments TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    confirmed_at INTEGER,
    UNIQUE(family_id, week_start, revision)
);
CREATE INDEX idx_weekly_plans_family_week ON weekly_plans(family_id, week_start);

CREATE TABLE shopping_lists (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plan_id INTEGER NOT NULL UNIQUE REFERENCES weekly_plans(id) ON DELETE CASCADE,
    week_start TEXT NOT NULL,
    plan_revision INTEGER NOT NULL,
    items TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX idx_shopping_lists_week ON shopping_lists(week_start);

CREATE TABLE school_menus (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    family_id TEXT NOT NULL,
    date TEXT NOT NULL,
    status TEXT NOT NULL,
    payload TEXT NOT NULL,
    source TEXT,
    imported_at INTEGER,
    origin TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(family_id, date)
);
CREATE INDEX idx_school_menus_family_date ON school_menus(family_id, date);
