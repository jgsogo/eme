-- Collapse plan revisions to one mutable plan per family/week. Preserve the latest
-- plan and only the shopping list that belonged to that plan.
CREATE TABLE weekly_plans_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    family_id TEXT NOT NULL,
    week_start TEXT NOT NULL CHECK(length(week_start) = 10),
    assignments TEXT NOT NULL CHECK(json_valid(assignments)),
    created_at INTEGER NOT NULL,
    UNIQUE(family_id, week_start)
);
INSERT INTO weekly_plans_new(id, family_id, week_start, assignments, created_at)
SELECT p.id, p.family_id, p.week_start, p.assignments, p.created_at
FROM weekly_plans p
WHERE p.id = (
    SELECT latest.id FROM weekly_plans latest
    WHERE latest.family_id = p.family_id AND latest.week_start = p.week_start
    ORDER BY latest.revision DESC, latest.id DESC LIMIT 1
);

CREATE TABLE shopping_lists_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    family_id TEXT NOT NULL,
    start_date TEXT NOT NULL CHECK(length(start_date) = 10),
    end_date TEXT NOT NULL CHECK(length(end_date) = 10),
    items TEXT NOT NULL CHECK(json_valid(items)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(family_id, start_date, end_date)
);
INSERT INTO shopping_lists_new(id, family_id, start_date, end_date, items, created_at, updated_at)
SELECT sl.id, p.family_id, p.week_start, date(p.week_start, '+6 days'), sl.items, sl.created_at, sl.updated_at
FROM shopping_lists sl
JOIN weekly_plans p ON p.id = sl.plan_id
WHERE p.id = (
    SELECT latest.id FROM weekly_plans latest
    WHERE latest.family_id = p.family_id AND latest.week_start = p.week_start
    ORDER BY latest.revision DESC, latest.id DESC LIMIT 1
);

DROP TABLE shopping_lists;
DROP TABLE weekly_plans;
ALTER TABLE weekly_plans_new RENAME TO weekly_plans;
ALTER TABLE shopping_lists_new RENAME TO shopping_lists;
CREATE INDEX idx_weekly_plans_family_week ON weekly_plans(family_id, week_start);
CREATE INDEX idx_shopping_lists_family_range ON shopping_lists(family_id, start_date, end_date);

-- School menus are always managed manually. Drop importer provenance columns.
CREATE TABLE school_menus_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    family_id TEXT NOT NULL,
    date TEXT NOT NULL CHECK(length(date) = 10),
    status TEXT NOT NULL CHECK(status IN ('known','not_published','closed','unknown')),
    payload TEXT NOT NULL CHECK(json_valid(payload)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(family_id, date)
);
INSERT INTO school_menus_new(id, family_id, date, status, payload, created_at, updated_at)
SELECT id, family_id, date, status, payload, created_at, updated_at FROM school_menus;
DROP TABLE school_menus;
ALTER TABLE school_menus_new RENAME TO school_menus;
CREATE INDEX idx_school_menus_family_date ON school_menus(family_id, date);
