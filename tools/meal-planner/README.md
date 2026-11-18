# meal-planner

The Meal Planner is an application module hosted inside the EME monorepo. It is
implemented as a Rust crate at `tools/meal-planner`, not as a separately
deployed service or a second web application.

## Responsibilities

The crate is the owner of Meal Planner domain rules, application use cases and
persistence as those capabilities are introduced. Its internal boundaries are
kept separate so domain and application logic can be tested without the agent
runtime, HTTP server or a live database.

The initial foundation provides:

- A configurable SQLite database owned by the Meal Planner module.
- Versioned, transactional schema migrations.
- A small storage boundary that can be shared by server-side adapters.
- Stable application error categories for validation, missing resources,
  conflicts and persistence failures.

The initial migration establishes module-owned metadata storage. Household,
recipe, school-menu, meal-plan and shopping-list tables belong in their
respective follow-up changes rather than being introduced before their domain
models are defined.

## Server integration

`agent-server` opens the database during startup and shares the store through
Actix application data for future API handlers. By default the database is
`<data-dir>/meal-planner.sqlite`; use `--meal-planner-database <path>` to
override it. The server reports the selected path and schema version through
its existing structured logging.

The crate does not register agent tools or expose HTTP routes yet. Those are
thin adapters to be added when the corresponding application use cases are
implemented. Agent authorization remains the responsibility of
`agent-runtime`, and the existing EME web console remains the UI.

## Development

The workspace CI runs formatting, checks and tests for this crate with the
rest of the workspace.

```bash
cargo test -p meal-planner
cargo check -p agent-server
```
