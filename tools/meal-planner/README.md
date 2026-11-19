# meal-planner

The Meal Planner is an application module hosted inside the EME monorepo, not a separately deployed service.

## Household and meal participation

The crate owns stable family-member identities, recurring participation rules and date-specific exceptions. The initial meal types are breakfast, lunch and dinner. Arrangements are home, school, away and not participating. Recurring rules are keyed by member, weekday and meal type and may have inclusive effective date bounds.

A date-specific exception overrides the recurring rule. When recurring rules overlap, the rule with the latest effective start wins; an unbounded start sorts first and the stable rule ID breaks ties. Missing configuration is distinct from an explicit not-participating choice.

The pure domain resolver can resolve one local calendar date or seven consecutive dates without depending on SQLite, HTTP or the agent runtime. The store provides CRUD operations and delegates date/week resolution to that domain function.

Member deactivation preserves identity and configuration. Deletion is allowed only when referential constraints permit it.

## Server integration

The server opens the SQLite database at startup and shares the store through Actix application data. The default database path is in the server data directory; use the --meal-planner-database option to override it.

## Development

Run cargo test -p meal-planner and cargo check -p agent-server. The household tests also cover recurring-rule precedence and date-specific overrides.

## Recipe catalogue and cooking

Recipes are provider-independent domain records. A recipe has a title, optional description/source/notes, serving count, structured ingredients, ordered preparation steps, categories/tags, dietary information and allergen information.

Ingredient amounts and units are optional and remain unknown when not supplied. No unit conversion or quantity inference is performed. Allergen information distinguishes an unknown value (`None`) from an explicitly checked empty list.

The SQLite recipe table stores a validated, versioned-by-migration JSON payload alongside searchable title and timestamps. The application boundary exposes create, replace, retrieve, list/search and delete operations through `MealPlannerStore`. Search is case-insensitive across title, description, ingredient names, categories and tags.

The authenticated server exposes the catalogue at `/meal-planner/recipes` and serves a responsive recipe page at `/console/recipes`. The cooking view keeps ingredients and preparation steps in their original order and lets the cook check them off locally; those checklist states are not persisted as recipe data.


## School lunch menus

School lunch menus are date-keyed records, separate from family recipes and household participation rules. Each date has an explicit state: known, not_published, closed or unknown. A missing record is distinct from every explicit state. Known menus contain at least one item; other states contain no dishes.

Menus are managed manually through create, retrieve, list, update and delete operations. Import endpoints, importer provenance and import-specific metadata are not part of the application.

## Weekly meal plans

A weekly plan is a mutable record keyed by family and Monday local calendar date. It contains date/meal slots with one or more participant assignments. Assignments snapshot member IDs and names, so later household participation edits do not silently rewrite the plan.

The authenticated API exposes GET, POST and PUT on `/meal-planner/plans/{week_start}`, plus DELETE on the same resource. Creating a plan from participation rules snapshots the resolved household participants; updates replace its assignments directly. There is no confirmation state, revision history or separate validation/confirmation workflow. Structural validation and family/recipe reference checks run whenever a plan is created or updated.

The responsive planner is available at `/console/meal-planner`. It supports month navigation, member selection, recipe selection, school-menu context and direct editing.

## Shopping lists

A shopping list is generated for a user-selected inclusive start/end date range, typically one week. It aggregates ingredients from every planned meal and recipe assignment whose meal date falls within the range. Planned servings are the number of participants, scaled against each recipe's base servings.

Ingredient aggregation is deterministic and conservative. Ingredient names are matched case-insensitively after whitespace normalization. Supported conversions are kilograms to grams, litres to millilitres, and common piece/unit aliases. Other units are combined only when normalized unit strings match. Ingredients without a unit remain separate. Missing quantities remain unknown; if any merged contribution has an unknown quantity, the consolidated quantity remains unknown.

Shopping lists are stored independently by family and date range. Users can add items, edit quantities, mark ingredients as already available at home, disable items they do not need to buy, and restore disabled or edited generated items. Regeneration preserves manual additions, exclusions, purchase state and quantity overrides.

The authenticated API exposes `GET /meal-planner/shopping-list` and `POST /meal-planner/shopping-list/generate` with `family_id`, `start_date` and `end_date`, plus item operations under `/meal-planner/shopping-list/{list_id}/items`.

## AI-assisted planning and EME agent tools

The server registers explicit, independently callable tools using the `domain.resource.action` naming convention:

- Recipes: `meals.recipe.create/get/list/update/delete`
- School menus: `meals.school_menu.create/get/list/update/delete`
- Plans: `meals.plan.get_context/create/get/list/update/delete`
- Shopping lists: `meals.shopping_list.get/generate/add_item/update_item/remove_item/restore_item`

Each action has its own tool definition and JSON schema; tools do not multiplex actions through an `operation` argument. Every tool requires authenticated user context and is scoped to the selected family. Recipe and plan integrity checks remain deterministic storage-boundary checks.