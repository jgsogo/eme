# RABEL catalogue integration

Rust adapter for the Salamanca Public Library (Casa de las Conchas) AbsysNet OPAC.

## Agent tools

- `rabel.search_catalog`: searches the catalogue and returns compact bibliographic matches. It does not fetch holdings for every result.
- `rabel.get_item`: fetches one selected record and its copy-level holdings.
- `rabel.list_libraries`: returns the library selector options exposed by RABEL.

The client follows AbsysNet's HTML meta-refresh into a session-specific URL and retains cookies in one reqwest client. A `record_ref` is an opaque, session-scoped link; pass it unchanged to `rabel.get_item`. It must not be constructed by the model.

Availability values are normalized to `available`, `loaned`, `reading_room_only`, `other`, or `unknown`. Unknown catalogue values are never treated as available. Every item response includes `checked_at`; availability is not cached.

## Limitations

The public OPAC is an undocumented HTML interface. This initial parser uses narrow markup heuristics and deliberately fails when search links cannot be recognized. Field-specific search and stable bibliographic identifiers are not enabled until their parameters are verified against this installation. Review saved HTML fixtures when the OPAC markup changes.

Run tests with `cargo test -p rabel-tool`.
