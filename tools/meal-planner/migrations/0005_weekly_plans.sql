CREATE TABLE weekly_plans (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    week_start TEXT NOT NULL CHECK(length(week_start) = 10),
    revision INTEGER NOT NULL CHECK(revision > 0),
    status TEXT NOT NULL CHECK(status IN ('draft', 'confirmed')),
    assignments TEXT NOT NULL CHECK(json_valid(assignments)),
    created_at INTEGER NOT NULL,
    confirmed_at INTEGER,
    UNIQUE(week_start, revision)
);
CREATE INDEX weekly_plans_week ON weekly_plans(week_start, revision DESC);
