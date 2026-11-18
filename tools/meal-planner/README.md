# meal-planner

The Meal Planner is an application module hosted inside the EME monorepo, not a separately deployed service.

## Household and meal participation

The crate owns stable family-member identities, recurring participation rules and date-specific exceptions. The initial meal types are breakfast, lunch and dinner. Arrangements are home, school, away and not participating. Recurring rules are keyed by member, weekday and meal type and may have inclusive effective date bounds.

A date-specific exception overrides the recurring rule. When recurring rules overlap, the rule with the latest effective start wins; an unbounded start sorts first and the stable rule ID breaks ties. Missing configuration is distinct from an explicit not-participating choice.

The pure domain resolver can resolve one local calendar date or seven consecutive dates without depending on SQLite, HTTP or the agent runtime. The store provides CRUD operations and delegates date/week resolution to that domain function.

Member deactivation preserves identity and configuration. Deletion is allowed only when referential constraints permit it. Confirmed-plan snapshots will be added with weekly planning so later rule edits cannot rewrite historical plans.

## Server integration

The server opens the SQLite database at startup and shares the store through Actix application data. The default database path is in the server data directory; use the --meal-planner-database option to override it.

## Development

Run cargo test -p meal-planner and cargo check -p agent-server. The household tests also cover recurring-rule precedence and date-specific overrides.

## Recipe catalogue and cooking

Recipes are provider-independent domain records. A recipe has a title, optional description/source/notes, serving count, structured ingredients, ordered preparation steps, categories/tags, dietary information and allergen information.

Ingredient amounts and units are optional and remain unknown when not supplied. No unit conversion or quantity inference is performed. Allergen information distinguishes an unknown value (`None`) from an explicitly checked empty list.

The SQLite recipe table stores a validated, versioned-by-migration JSON payload alongside searchable title and timestamps. The application boundary exposes create, replace, retrieve, list/search and delete operations through `MealPlannerStore`. Search is case-insensitive across title, description, ingredient names, categories and tags.

The authenticated server exposes the catalogue at `/meal-planner/recipes` and serves a responsive recipe page at `/console/recipes`. The cooking view keeps ingredients and preparation steps in their original order and lets the cook check them off locally; those checklist states are not persisted as recipe data.
