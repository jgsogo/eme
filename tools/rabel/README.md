# RABEL catalogue integration

Rust adapter for the Salamanca Public Library (Casa de las Conchas) AbsysNet OPAC.

## Agent tools

- `rabel.search_catalog`: searches the catalogue and returns compact bibliographic matches. It does not fetch holdings for every result.
- `rabel.get_item`: fetches bibliographic metadata for one selected record, including title responsibility, publication statement/year, physical description, series, notes, subjects and ISBN.

Record parsing prioritizes the structured `#docu_body` definition lists. Publication years are extracted from publication/editor fields rather than from arbitrary page text, so author life dates are not mistaken for edition dates.
- `rabel.list_libraries`: returns the library selector options exposed by RABEL.

The authenticated assistant also has collection-aware tools registered by `books-tool`:

- `books.list_libraries` and `books.set_preferred_libraries`: inspect and update the current user's selected libraries.
- `books.check_availability`: resolve a saved book independently in every selected library and fetch live copy-level holdings, including per-library status and check time.
- `books.refresh_catalogue`: refresh bibliographic metadata for a saved book without changing personal collection fields.
- `books.list/get/add/update/remove`: manage the user's own collection. Tool access is scoped to the authenticated `ToolContext.user_id`.

Availability is never inferred from a search result or bibliographic record. A failed or ambiguous lookup is reported as an error/not-found result rather than as an available copy.

The client follows AbsysNet's HTML meta-refresh into a session-specific URL and retains cookies in one reqwest client. A `record_ref` is an opaque, session-scoped link; pass it unchanged to `rabel.get_item` only when using the same shared client/session. Do not persist it or pass it between independent HTTP requests. The web UI re-runs the search and resolves the selected record within the same request/session.

## Diagnostic CLI

The `rabel` binary uses the same client and parser as the agent integration. It is useful for investigating catalogue responses, creating parser fixtures, and inspecting records without running the agent server.

```sh
# Parsed search results as JSON (default)
cargo run -p rabel-tool --bin rabel -- search "Los fundamentos de la libertad"

# Exact HTML returned by the search request
cargo run -p rabel-tool --bin rabel -- search "Los fundamentos de la libertad" --format html

# Parsed bibliographic details for the first search result
cargo run -p rabel-tool --bin rabel -- record "Los fundamentos de la libertad"

# Raw HTML for that record's detail page
cargo run -p rabel-tool --bin rabel -- record "Los fundamentos de la libertad" --format html

# Select another search result or catalogue
cargo run -p rabel-tool --bin rabel -- --subcatalog SAL/BPSA record "Hayek libertad" --index 1
```

Supported formats are currently `json` and `html`. HTML mode prints the upstream response unchanged to stdout; errors are written to stderr. Record lookup searches and fetches the selected result within the same process because RABEL record references are session-scoped.

## Availability

Availability is fetched separately from bibliographic metadata. Values are normalized to `available`, `loaned`, `reading_room_only`, `other`, or `unknown`. Unknown catalogue values are never treated as available. Every holdings response includes `checked_at`; availability is not cached.

## Limitations

The public OPAC is an undocumented HTML interface. The parser prioritizes the structured bibliographic definition lists and uses conservative fallbacks for alternate templates. Review saved HTML fixtures when the OPAC markup changes. The personal collection API also exposes an authenticated `POST /books/{id}/refresh-catalogue` action that re-fetches metadata by ISBN, then exact parsed-title match, without changing personal reading/rating fields.

Run tests with `cargo test -p rabel-tool`.
