# Repository guidance

## Rust crate structure

- Put every trait, struct, and enum in its own source file.
- Group files into modules that isolate responsibilities. Add brief module-level documentation explaining each module's responsibility.
- Keep visibility decisions at module boundaries. Prefer re-exporting the items a caller needs over complex visibility rules such as `pub(crate)`.
- Export only items consumed by other crates or modules. Keep implementation details private.
- Do not preserve backwards-compatibility shims or aliases: this repository has no external crate consumers. Prefer the simplest design and break internal interfaces when useful. Ask the human only when a design decision is genuinely ambiguous.
- The crate-root `src/lib.rs` must document the crate's purpose in at least two or three short paragraphs. Explain the responsibility of the crate, the functionality it provides, and (where relevant) the boundaries of what belongs elsewhere.
- In every Rust source file, order content as follows: all `mod` declarations first, a blank line, all `use` declarations, a blank line, then the file content.

## Website landing pages

The project has two localized versions of the public landing page:

- English (default): `website/index.html` — deployed at `https://jgsogo.es/eme/`.
- Spanish: `website/es/index.html` — deployed at `https://jgsogo.es/eme/es/`.

Keep both pages synchronized whenever either one changes:

- Treat them as two language variants of the same page, not as independent designs.
- Preserve the same section order, information architecture, semantic HTML structure, responsive behavior, CSS, visual styling, navigation targets, and functional links.
- Translate all user-facing copy, metadata (`lang`, title, description), accessible labels, and language-specific navigation text. Keep product names, code identifiers, URLs, and technical terms accurate.
- Add new sections, features, links, or visual changes to both versions in the same change.
- Keep the language switch visible on both pages: English links to `es/`; Spanish links to `../`. English remains the default at the root URL.
- The Spanish page uses relative paths from `website/es/`; for example, the Rust API documentation link must point to `../docs/rust/`.
- Do not let translation alter technical meaning or imply that planned capabilities are already implemented. Keep project status and capability claims equivalent in both languages.
- When editing either landing page, review the counterpart in the same task and update it before considering the work complete.
