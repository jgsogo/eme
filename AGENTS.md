# Repository guidance

## Rust crate structure

- Put every trait, struct, and enum in its own source file.
- Group files into modules that isolate responsibilities. Add brief module-level documentation explaining each module's responsibility.
- Keep visibility decisions at module boundaries. Prefer re-exporting the items a caller needs over complex visibility rules such as `pub(crate)`.
- Export only items consumed by other crates or modules. Keep implementation details private.
- Do not preserve backwards-compatibility shims or aliases: this repository has no external crate consumers. Prefer the simplest design and break internal interfaces when useful. Ask the human only when a design decision is genuinely ambiguous.
- The crate-root `src/lib.rs` must document the crate's purpose in at least two or three short paragraphs. Explain the responsibility of the crate, the functionality it provides, and (where relevant) the boundaries of what belongs elsewhere.
- In every Rust source file, order content as follows: all `mod` declarations first, a blank line, all `use` declarations, a blank line, then the file content.
