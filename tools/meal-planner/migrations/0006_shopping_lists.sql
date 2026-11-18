CREATE TABLE shopping_lists (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plan_id INTEGER NOT NULL UNIQUE REFERENCES weekly_plans(id) ON DELETE CASCADE,
    week_start TEXT NOT NULL CHECK(length(week_start) = 10),
    plan_revision INTEGER NOT NULL CHECK(plan_revision > 0),
    items TEXT NOT NULL CHECK(json_valid(items)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX shopping_lists_week ON shopping_lists(week_start, plan_revision DESC, id DESC);
