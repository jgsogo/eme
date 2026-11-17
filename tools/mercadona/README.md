# mercadona-tool

Unofficial Mercadona product catalogue and search tools for the EME agent platform.

The crate currently exposes anonymous, read-only operations against the Mercadona online shop and its Algolia product-search index:

- `mercadona_search_products`: search the product catalogue.
- `mercadona_get_product`: retrieve product details.
- `mercadona_list_categories`: retrieve the category tree.
- `mercadona_get_category`: retrieve a category and its products.
- `mercadona_resolve_postal_code`: resolve a Spanish postal code to the serving warehouse.

The integration is intentionally read-only. Authentication, cart mutations, checkout and order submission are not implemented.

## Configuration

All configuration is supplied outside model-controlled tool arguments:

- `MERCADONA_WAREHOUSE`: default warehouse code (defaults to `mad1`).
- `MERCADONA_LANG`: catalogue language (defaults to `es`).
- `MERCADONA_ALGOLIA_APP_ID` and `MERCADONA_ALGOLIA_API_KEY`: optional Algolia search credentials. When omitted, the client discovers the current public search credentials from the Mercadona SPA shell and JavaScript bundle.

A tool may also receive an optional `warehouse` argument to query a different catalogue. This selects a public catalogue only; it does not change account or delivery settings.

## API reference

The implementation is based on reverse engineering and API observations from [ivorpad/mercadona-cli](https://github.com/ivorpad/mercadona-cli). That project is the reference for the unofficial Mercadona shop API, its Algolia integration, endpoint paths, request headers and warehouse resolution.

Mercadona does not publish this as a supported public API. Endpoints, response formats, headers, and Algolia credentials can change without notice. The integration uses only anonymous catalogue/search endpoints and should be treated as best-effort.

## Boundaries

- This crate owns Mercadona HTTP interactions and the tools exposed to agents.
- `agent-runtime` owns tool validation, authorization, dispatch and execution lifecycle.
- Credentials and service configuration are read from the environment, never supplied by the model.
- The crate does not implement account authentication, cart changes, delivery booking, checkout or purchases.
