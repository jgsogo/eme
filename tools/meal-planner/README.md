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


## School lunch menus

School lunch menus are date-keyed records, separate from family recipes and household participation rules. Each date has an explicit state: known, not_published, closed or unknown. A missing database record is distinct from every explicit state. A known menu must contain at least one item; other states cannot contain dishes.

Menu items retain optional notes, allergen annotations (unknown differs from a checked empty list) and dietary alternatives. Records retain source, import timestamp and origin. Manual entries and corrections take precedence: imports skip dates already marked manual, while later imports may refresh earlier imported records.

The application supports manual save/edit, date and inclusive range retrieval, deletion, and a provider-neutral batch import boundary. No public school-menu API or provider format is assumed; a concrete importer should be added only after its source and format are identified. Holiday calendars remain separate context and are not used to infer that a menu is absent or a school is closed.


## Weekly meal plans

A weekly plan is keyed by a Monday local calendar date and contains date/meal slots with one or more participant assignments. Assignments snapshot member IDs and names, so later edits to household participation rules do not silently change an existing plan. The initial draft is generated from the participation resolver and groups members who share the same arrangement.

Assignments distinguish home, school, away and not-participating arrangements. Home assignments may reference a recipe. School assignments reference the school-menu date and never a family recipe; if no menu record exists, the UI reports that no menu is recorded rather than inferring that the school is closed.

Plans have explicit draft and confirmed states. Drafts can be edited; confirmation records a timestamp and makes that revision immutable. Creating a revision copies the latest confirmed assignments into a new draft revision. Earlier confirmed revisions remain available through the revision-history API.

The authenticated API exposes:
- `GET /meal-planner/plans/{week_start}` and `GET /meal-planner/plans/{week_start}/revisions`
- `POST /meal-planner/plans/{week_start}` to create a draft from current participation rules
- `PUT /meal-planner/plans/{week_start}` to replace the latest draft assignments
- `POST /meal-planner/plans/{week_start}/confirm` and `POST /meal-planner/plans/{week_start}/revise`
- `GET /meal-planner/members` and `POST /meal-planner/members` for planner participant selection

The responsive planner is available at `/console/meal-planner`. It supports week navigation, member creation, multiple assignments per slot, recipe selection for home-prepared meals, school-menu context, draft editing and explicit confirmation/revision.


## Shopping lists

A shopping list is generated from a specific weekly-plan revision. Only home-prepared assignments with a recipe contribute ingredients; school, away and not-participating assignments are excluded. Planned servings are the number of participants in the assignment, scaled against the recipe's base servings.

Ingredient aggregation is deterministic and conservative. Ingredient names are matched case-insensitively after whitespace normalization. Supported conversions are kilograms to grams, litres to millilitres, and common piece/unit aliases. Other units are combined only when their normalized unit strings match. Ingredients with no unit are kept separate because their quantities cannot be safely compared. Missing quantities remain unknown; if any merged contribution with a known compatible unit has an unknown quantity, the consolidated quantity remains unknown.

A list is stored separately from the plan and can be edited independently. Manual additions, explicit removals, purchased status and quantity overrides are reconciled when the user explicitly regenerates the list. When a new immutable plan revision is selected, generation creates a separate list snapshot seeded from the latest earlier list for that week; earlier list snapshots remain available against their original plan revisions. Generated entries no longer present in the new plan are retained as stale when they contain user edits or purchase state, so they can be reviewed rather than silently discarded.

The authenticated API exposes:
- `GET /meal-planner/plans/{plan_id}/shopping-list`
- `POST /meal-planner/plans/{plan_id}/shopping-list/generate` to generate or explicitly regenerate from that exact plan revision
- `POST /meal-planner/plans/{plan_id}/shopping-list/items` to add a manual item
- `PUT /meal-planner/plans/{plan_id}/shopping-list/items/{item_id}` to edit an item or mark it purchased
- `DELETE /meal-planner/plans/{plan_id}/shopping-list/items/{item_id}` to remove an item (generated items are retained as removal markers for future regeneration)
