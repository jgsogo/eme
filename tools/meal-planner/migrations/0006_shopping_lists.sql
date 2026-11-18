CREATE TABLE shopping_lists (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plan_id INTEGER NOT NULL UNIQUE REFERENCES weekly_plans(id) ON DELETE CASCADE,
    plan_revision INTEGER NOT NULL CHECK(plan_revision > 0),
    items TEXT NOT NULL CHECK(json_valid(items)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX shopping_lists_plan_revision ON shopping_lists(plan_id, plan_revision);
