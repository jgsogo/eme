-- Breakfast is no longer a supported participation meal. Remove existing
-- breakfast configuration and constrain future writes to lunch and dinner.
CREATE TABLE recurring_participation_rules_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    family_id TEXT NOT NULL,
    member_id INTEGER NOT NULL REFERENCES family_members(id) ON DELETE RESTRICT,
    weekday INTEGER NOT NULL CHECK(weekday BETWEEN 0 AND 6),
    meal_type TEXT NOT NULL CHECK(meal_type IN ('lunch', 'dinner')),
    arrangement TEXT NOT NULL,
    effective_from TEXT,
    effective_until TEXT
);
INSERT INTO recurring_participation_rules_new
    (id, family_id, member_id, weekday, meal_type, arrangement, effective_from, effective_until)
SELECT id, family_id, member_id, weekday, meal_type, arrangement, effective_from, effective_until
FROM recurring_participation_rules
WHERE meal_type IN ('lunch', 'dinner');
DROP TABLE recurring_participation_rules;
ALTER TABLE recurring_participation_rules_new RENAME TO recurring_participation_rules;
CREATE INDEX idx_participation_rules_family
    ON recurring_participation_rules(family_id, member_id);

CREATE TABLE meal_participation_exceptions_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    family_id TEXT NOT NULL,
    member_id INTEGER NOT NULL REFERENCES family_members(id) ON DELETE RESTRICT,
    date TEXT NOT NULL,
    meal_type TEXT NOT NULL CHECK(meal_type IN ('lunch', 'dinner')),
    arrangement TEXT NOT NULL,
    UNIQUE(family_id, member_id, date, meal_type)
);
INSERT INTO meal_participation_exceptions_new
    (id, family_id, member_id, date, meal_type, arrangement)
SELECT id, family_id, member_id, date, meal_type, arrangement
FROM meal_participation_exceptions
WHERE meal_type IN ('lunch', 'dinner');
DROP TABLE meal_participation_exceptions;
ALTER TABLE meal_participation_exceptions_new RENAME TO meal_participation_exceptions;
CREATE INDEX idx_participation_exceptions_family
    ON meal_participation_exceptions(family_id, date);
