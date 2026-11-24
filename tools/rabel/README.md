# RABEL catalogue integration

Rust adapter for the Salamanca Public Library (Casa de las Conchas) AbsysNet OPAC.

## Agent tools

- `rabel.search_catalog`: searches the catalogue and returns compact bibliographic matches. It does not fetch holdings for every result.
- `rabel.get_item`: fetches bibliographic metadata for one selected record.
- `rabel.list_libraries`: returns the library selector options exposed by RABEL.

The client follows AbsysNet's HTML meta-refresh into a session-specific URL and retains cookies in one reqwest client. A `record_ref` is an opaque, session-scoped link; pass it unchanged to `rabel.get_item`. It must not be constructed by the model.

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

The public OPAC is an undocumented HTML interface. The parser uses narrow markup heuristics and deliberately fails when search links cannot be recognized. Review saved HTML fixtures when the OPAC markup changes.

Run tests with `cargo test -p rabel-tool`.
