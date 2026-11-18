-- Initial Meal Planner schema baseline.
--
-- Domain tables are introduced by the household, recipe, school-menu and
-- planning changes once their ownership and invariants are defined.
CREATE TABLE meal_planner_metadata (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
